use pariter::IteratorExt;

use crate::config::{Config, Delimiter};
use crate::moonblade::Program;
use crate::util;
use crate::CliResult;

static USAGE: &str = r#"
The filter command evaluates an expression for each row of the given CSV file and
only output the row if the result of beforementioned expression is truthy.

For instance, given the following CSV file:

a
1
2
3

The following command:

    $ xan filter 'a > 1'

Will produce the following result:

a
2
3

For a quick review of the capabilities of the expression language,
check out the `xan help cheatsheet` command.

For a list of available functions, use `xan help functions`.

Usage:
    xan filter [options] <expression> [<input>]
    xan filter --help

filter options:
    -p, --parallel             Whether to use parallelization to speed up computations.
                               Will automatically select a suitable number of threads to use
                               based on your number of cores. Use -t, --threads if you want to
                               indicate the number of threads yourself.
    -t, --threads <threads>    Parellize computations using this many threads. Use -p, --parallel
                               if you want the number of threads to be automatically chosen instead.
    -v, --invert-match         If set, will invert the evaluated value.
    -l, --limit <n>            Maximum number of rows to return. Useful to avoid downstream
                               buffering some times (e.g. when searching for very few
                               rows in a big file before piping to `view` or `flatten`).
                               Does not work when parallelizing.

Common options:
    -h, --help               Display this message
    -o, --output <file>      Write output to <file> instead of stdout.
    -n, --no-headers         When set, the first row will not be evaled
                             as headers.
    -d, --delimiter <arg>    The field delimiter for reading CSV data.
                             Must be a single character.
"#;

#[derive(Deserialize)]
struct Args {
    arg_expression: String,
    arg_input: Option<String>,
    flag_output: Option<String>,
    flag_no_headers: bool,
    flag_delimiter: Option<Delimiter>,
    flag_parallel: bool,
    flag_limit: Option<usize>,
    flag_threads: Option<usize>,
    flag_invert_match: bool,
}

pub fn run(argv: &[&str]) -> CliResult<()> {
    let args: Args = util::get_args(USAGE, argv)?;
    let rconf = Config::new(&args.arg_input)
        .no_headers(args.flag_no_headers)
        .delimiter(args.flag_delimiter);

    let parallelization = match (args.flag_parallel, args.flag_threads) {
        (true, None) => Some(None),
        (_, Some(count)) => Some(Some(count)),
        _ => None,
    };

    let mut wtr = Config::new(&args.flag_output).writer()?;

    let mut rdr = rconf.reader()?;
    let headers = rdr.byte_headers()?.clone();

    rconf.write_headers(&mut rdr, &mut wtr)?;

    let program = Program::parse(&args.arg_expression, &headers)?;
    let mut matches: usize = 0;

    if let Some(threads) = parallelization {
        for result in rdr.into_byte_records().enumerate().parallel_map_custom(
            |o| o.threads(threads.unwrap_or_else(num_cpus::get)),
            move |(index, record)| -> CliResult<Option<csv::ByteRecord>> {
                let record = record?;

                let value = program.run_with_record(index, &record)?;

                let mut is_match = value.is_truthy();

                if args.flag_invert_match {
                    is_match = !is_match;
                }

                Ok(is_match.then_some(record))
            },
        ) {
            if let Some(record) = result? {
                matches += 1;
                wtr.write_byte_record(&record)?;
            }

            if let Some(limit) = args.flag_limit {
                if matches >= limit {
                    break;
                }
            }
        }
    } else {
        let mut record = csv::ByteRecord::new();
        let mut index: usize = 0;

        while rdr.read_byte_record(&mut record)? {
            let value = program.run_with_record(index, &record)?;

            let mut is_match = value.is_truthy();

            if args.flag_invert_match {
                is_match = !is_match;
            }

            if is_match {
                matches += 1;
                wtr.write_byte_record(&record)?;
            }

            if let Some(limit) = args.flag_limit {
                if matches >= limit {
                    break;
                }
            }

            index += 1;
        }
    }

    Ok(wtr.flush()?)
}
