use std::fs;

use crate::config::{Config, Delimiter};
use crate::index::Indexed;
use crate::util;
use crate::CliResult;

static USAGE: &str = "
Returns the rows in the range specified (starting at 0, half-open interval).
The range does not include headers.

If the start of the range isn't specified, then the slice starts from the first
record in the CSV data.

If the end of the range isn't specified, then the slice continues to the last
record in the CSV data.

This operation can be made much faster by creating an index with 'xan index'
first. Namely, a slice on an index requires parsing just the rows that are
sliced. Without an index, all rows up to the first row in the slice must be
parsed.

Usage:
    xan slice [options] [<input>]

slice options:
    -s, --start <n>  The index of the record to slice from.
    -e, --end <n>    The index of the record to slice to.
    -l, --len <n>    The length of the slice (can be used instead
                     of --end).
    -i, --index <i>  Slice a single record (shortcut for -s N -l 1).
                     You can also provide multiples indices separated by
                     commas, e.g. \"1,4,67,89\". Note that selected records
                     will be emitted in file order.

Common options:
    -h, --help             Display this message
    -o, --output <file>    Write output to <file> instead of stdout.
    -n, --no-headers       When set, the first row will not be interpreted
                           as headers. Otherwise, the first row will always
                           appear in the output as the header row.
    -d, --delimiter <arg>  The field delimiter for reading CSV data.
                           Must be a single character.
";

#[derive(Deserialize)]
struct Args {
    arg_input: Option<String>,
    flag_start: Option<usize>,
    flag_end: Option<usize>,
    flag_len: Option<usize>,
    flag_index: Option<String>,
    flag_output: Option<String>,
    flag_no_headers: bool,
    flag_delimiter: Option<Delimiter>,
}

pub fn run(argv: &[&str]) -> CliResult<()> {
    let args: Args = util::get_args(USAGE, argv)?;

    match &args.flag_index {
        Some(indices) if indices.contains(',') => {
            return match args.rconfig().indexed()? {
                None => args.no_index_plural(),
                Some(idx) => args.with_index_plural(idx),
            };
        }
        _ => (),
    };

    match args.rconfig().indexed()? {
        None => args.no_index(),
        Some(idx) => args.with_index(idx),
    }
}

impl Args {
    fn no_index(&self) -> CliResult<()> {
        let mut rdr = self.rconfig().reader()?;
        let mut wtr = self.wconfig().writer()?;
        self.rconfig().write_headers(&mut rdr, &mut wtr)?;

        let (start, end) = self.range()?;
        for r in rdr.byte_records().skip(start).take(end - start) {
            wtr.write_byte_record(&r?)?;
        }
        Ok(wtr.flush()?)
    }

    fn with_index(&self, mut idx: Indexed<fs::File, fs::File>) -> CliResult<()> {
        let mut wtr = self.wconfig().writer()?;
        self.rconfig().write_headers(&mut *idx, &mut wtr)?;

        let (start, end) = self.range()?;
        if end - start == 0 {
            return Ok(());
        }
        idx.seek(start as u64)?;
        for r in idx.byte_records().take(end - start) {
            wtr.write_byte_record(&r?)?;
        }
        wtr.flush()?;
        Ok(())
    }

    fn no_index_plural(&self) -> CliResult<()> {
        let mut rdr = self.rconfig().reader()?;
        let mut wtr = self.wconfig().writer()?;
        self.rconfig().write_headers(&mut rdr, &mut wtr)?;

        let indices = self.plural_indices()?;

        let mut record = csv::ByteRecord::new();
        let mut i: usize = 0;

        while rdr.read_byte_record(&mut record)? {
            if indices.contains(&i) {
                wtr.write_byte_record(&record)?;
            }

            i += 1;

            if &i > indices.last().unwrap() {
                break;
            }
        }

        Ok(wtr.flush()?)
    }

    fn with_index_plural(&self, mut idx: Indexed<fs::File, fs::File>) -> CliResult<()> {
        let mut wtr = self.wconfig().writer()?;
        self.rconfig().write_headers(&mut *idx, &mut wtr)?;

        for index in self.plural_indices()? {
            idx.seek(index as u64)?;

            match idx.byte_records().next() {
                None => break,
                Some(record) => {
                    wtr.write_byte_record(&record?)?;
                }
            }
        }

        Ok(wtr.flush()?)
    }

    fn range(&self) -> Result<(usize, usize), String> {
        let index: Option<usize> = self
            .flag_index
            .as_ref()
            .map(|string| string.parse::<usize>())
            .transpose()
            .map_err(|_| "could not parse -i/--index!")?;

        util::range(self.flag_start, self.flag_end, self.flag_len, index)
    }

    // NOTE: there is room to optimize, but this seems pointless currently
    fn plural_indices(&self) -> Result<Vec<usize>, &str> {
        self.flag_index
            .as_ref()
            .unwrap()
            .split(',')
            .map(|string| {
                string
                    .parse::<usize>()
                    .map_err(|_| "could not parse some index in -i/--index!")
            })
            .collect::<Result<Vec<usize>, _>>()
            .map(|mut indices| {
                indices.sort();
                indices
            })
    }

    fn rconfig(&self) -> Config {
        Config::new(&self.arg_input)
            .delimiter(self.flag_delimiter)
            .no_headers(self.flag_no_headers)
    }

    fn wconfig(&self) -> Config {
        Config::new(&self.flag_output)
    }
}
