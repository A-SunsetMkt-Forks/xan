use std::fs::File;
use std::io::{self, Read};
use std::iter;
use std::path::PathBuf;

use bstr::ByteSlice;
use colored::Colorize;
use flate2::read::GzDecoder;
use lazy_static::lazy_static;
use pariter::IteratorExt;
use regex::bytes::Regex;
use scraper::{Html, Selector};

use crate::config::{Config, Delimiter};
use crate::moonblade::{DynamicValue, ScrapingProgram};
use crate::select::{SelectColumns, Selection};
use crate::util;
use crate::CliResult;

// IO helpers
fn open(input_dir: &str, filename: &str) -> io::Result<Box<dyn Read>> {
    let mut path = PathBuf::from(input_dir);
    path.push(filename);

    let file = File::open(path)?;

    Ok(if filename.ends_with(".gz") {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    })
}

fn read_string(input_dir: &str, filename: &str) -> io::Result<String> {
    let mut string = String::new();

    open(input_dir, filename)?.read_to_string(&mut string)?;

    Ok(string)
}

const PREBUFFER_SIZE: usize = 65536;

fn read_max_bytes(input_dir: &str, filename: &str, max: u64) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::with_capacity(max as usize);

    open(input_dir, filename)?
        .take(max)
        .read_to_end(&mut buffer)?;

    Ok(buffer)
}

fn read_html(input_dir: &str, filename: &str) -> io::Result<Html> {
    read_string(input_dir, filename).map(|string| Html::parse_document(&string))
}

enum ScraperTarget<'a> {
    HtmlCell(&'a [u8]),
    HtmlFile(&'a str, &'a str),
}

fn guard_invalid_html_cell(cell: &[u8]) -> CliResult<()> {
    if !cell.is_empty() && !looks_like_html(cell) {
        Err(format!(
            "encountered cell value that does not look like HTML: {}!\nDid you forget to give {}?",
            std::str::from_utf8(cell).unwrap().green(),
            "-I/--input-dir".cyan()
        ))?
    } else {
        Ok(())
    }
}

impl ScraperTarget<'_> {
    fn prebuffer_bytes(&self) -> CliResult<Vec<u8>> {
        match self {
            Self::HtmlCell(cell) => {
                guard_invalid_html_cell(cell)?;

                Ok(cell[..PREBUFFER_SIZE.min(cell.len())].to_vec())
            }
            Self::HtmlFile(input_dir, filename) => {
                Ok(read_max_bytes(input_dir, filename, PREBUFFER_SIZE as u64)?)
            }
        }
    }

    fn read_html(&self) -> CliResult<Html> {
        match self {
            Self::HtmlCell(cell) => {
                guard_invalid_html_cell(cell)?;

                Ok(Html::parse_document(
                    std::str::from_utf8(cell).expect("invalid utf-8"),
                ))
            }
            Self::HtmlFile(input_dir, filename) => Ok(read_html(input_dir, filename)?),
        }
    }
}

// Scraper abstractions
lazy_static! {
    static ref HTML_LIKE_REGEX: Regex =
        Regex::new(r"^\s*<(?:html|head|body|title|meta|link|span|div|img|ul|ol|[ap!?])").unwrap();
    static ref TITLE_REGEX: Regex = Regex::new(r"<title>(.*?)</title>").unwrap();
    static ref URLS_IN_HTML_REGEX: Regex =
        Regex::new(r#"<a[^>]*\shref=(?:"([^"]*)"|'([^']*)'|([^\s>]*))[^>]*>"#).unwrap();
}

fn looks_like_html(bytes: &[u8]) -> bool {
    HTML_LIKE_REGEX.is_match(bytes)
}

#[derive(Clone)]
struct CustomScraper {
    program: ScrapingProgram,
    foreach: Option<Selector>,
}

impl CustomScraper {
    fn scrape(
        &self,
        index: usize,
        record: &csv::ByteRecord,
        target: ScraperTarget,
    ) -> CliResult<Vec<Vec<DynamicValue>>> {
        let html = target.read_html()?;

        if let Some(selector) = &self.foreach {
            Ok(self
                .program
                .run_plural(index, record, &html, selector)
                .collect::<Result<Vec<_>, _>>()?)
        } else {
            Ok(vec![self.program.run_singular(index, record, &html)?])
        }
    }
}

#[derive(Clone)]
enum Scraper {
    Title,
    Custom(CustomScraper),
}

impl Scraper {
    fn names(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match self {
            Self::Title => Box::new(iter::once("title")),
            Self::Custom(scraper) => Box::new(scraper.program.names()),
        }
    }

    fn scrape_title(&self, target: ScraperTarget) -> CliResult<Option<DynamicValue>> {
        let bytes = target.prebuffer_bytes()?;

        debug_assert!(bytes.len() <= PREBUFFER_SIZE);

        Ok(TITLE_REGEX.captures(&bytes).map(|caps| {
            DynamicValue::from(html_escape::decode_html_entities(
                std::str::from_utf8(&caps[1]).unwrap(),
            ))
        }))
    }

    fn scrape(
        &self,
        index: usize,
        record: &csv::ByteRecord,
        target: ScraperTarget,
    ) -> CliResult<Vec<Vec<DynamicValue>>> {
        match self {
            Self::Title => {
                let title_opt = self.scrape_title(target)?;

                Ok(vec![vec![title_opt.unwrap_or(DynamicValue::None)]])
            }
            Self::Custom(scraper) => scraper.scrape(index, record, target),
        }
    }
}

static USAGE: &str = "
Scrape HTML using a CSS-like expression language.

TODO...

Usage:
    xan scrape -e <expr> <column> [options] [<input>]
    xan scrape title <column> [options] [<input>]
    xan scrape --help

scrape options:
    -e, --evaluate <expr>    If given, evaluate the given scraping expression.
    -I, --input-dir <path>   If given, target column will be understood
                             as relative path to read from this input
                             directory instead.
    -k, --keep <column>      Selection of columns from the input to keep in
                             the output.
    -p, --parallel           Whether to use parallelization to speed up computations.
                             Will automatically select a suitable number of threads to use
                             based on your number of cores. Use -t, --threads if you want to
                             indicate the number of threads yourself.
    -t, --threads <threads>  Parellize computations using this many threads. Use -p, --parallel
                             if you want the number of threads to be automatically chosen instead.

scrape -e/--evaluate options:
    -f, --foreach <css>  If given, will return one row per element matching
                         the CSS selector in target document, instead of returning
                         a single row per document.
    --sep <char>            Separator to use when serializing lists.
                         [default: |]

Common options:
    -h, --help             Display this message
    -o, --output <file>    Write output to <file> instead of stdout.
    -n, --no-headers       When set, the first row will not be interpreted
                           as headers.
    -d, --delimiter <arg>  The field delimiter for reading CSV data.
                           Must be a single character.
";

#[derive(Deserialize)]
struct Args {
    arg_input: Option<String>,
    arg_column: SelectColumns,
    cmd_title: bool,
    flag_delimiter: Option<Delimiter>,
    flag_output: Option<String>,
    flag_no_headers: bool,
    flag_evaluate: Option<String>,
    flag_foreach: Option<String>,
    flag_input_dir: Option<String>,
    flag_keep: Option<SelectColumns>,
    flag_sep: String,
    flag_parallel: bool,
    flag_threads: Option<usize>,
}

pub fn run(argv: &[&str]) -> CliResult<()> {
    let args: Args = util::get_args(USAGE, argv)?;
    let conf = Config::new(&args.arg_input)
        .delimiter(args.flag_delimiter)
        .select(args.arg_column)
        .no_headers(args.flag_no_headers);

    let mut reader = conf.reader()?;
    let headers = reader.byte_headers()?.clone();
    let column_index = conf.single_selection(&headers)?;

    let parallelization = match (args.flag_parallel, args.flag_threads) {
        (true, None) => Some(None),
        (_, Some(count)) => Some(Some(count)),
        _ => None,
    };

    if args.flag_evaluate.is_none() && args.flag_foreach.is_some() {
        Err("--foreach only works with -e/--evaluate!")?;
    }

    let scraper = match &args.flag_evaluate {
        Some(code) => Scraper::Custom(CustomScraper {
            program: ScrapingProgram::parse(code, &headers)?,
            foreach: args
                .flag_foreach
                .as_ref()
                .map(|css| {
                    Selector::parse(css).map_err(|_| format!("invalid CSS selector: {}", css))
                })
                .transpose()?,
        }),
        None => {
            if args.cmd_title {
                Scraper::Title
            } else {
                unreachable!()
            }
        }
    };

    let keep = args
        .flag_keep
        .map(|s| {
            if s.is_empty() {
                Ok(Selection::empty())
            } else {
                s.selection(&headers, !args.flag_no_headers)
            }
        })
        .transpose()?;

    let mut writer = Config::new(&args.flag_output).writer()?;

    let padding = scraper.names().count();

    if !args.flag_no_headers {
        let mut output_headers = headers.clone();

        if let Some(keep_sel) = &keep {
            output_headers = keep_sel.select(&output_headers).collect();
        }

        for name in scraper.names() {
            output_headers.push_field(name.as_bytes());
        }

        writer.write_byte_record(&output_headers)?;
    }

    if let Some(threads) = parallelization {
        reader
            .into_byte_records()
            .enumerate()
            .parallel_map_custom(
                |o| {
                    if let Some(count) = threads {
                        o.threads(count)
                    } else {
                        o
                    }
                },
                move |(index, result)| -> CliResult<(csv::ByteRecord, Vec<Vec<DynamicValue>>)> {
                    let record = result?;

                    let cell = &record[column_index];

                    if cell.trim().is_empty() {
                        return Ok((record, vec![vec![DynamicValue::None; padding]]));
                    }

                    let target = if let Some(input_dir) = &args.flag_input_dir {
                        ScraperTarget::HtmlFile(
                            input_dir,
                            std::str::from_utf8(cell).expect("invalid utf-8"),
                        )
                    } else {
                        ScraperTarget::HtmlCell(cell)
                    };

                    let output_rows = scraper.scrape(index, &record, target)?;

                    Ok((record, output_rows))
                },
            )
            .try_for_each(|result| -> CliResult<()> {
                let (record, output_rows) = result?;

                for output_row in output_rows {
                    let mut output_record = if let Some(keep_sel) = &keep {
                        keep_sel.select(&record).collect()
                    } else {
                        record.clone()
                    };

                    for value in output_row {
                        output_record.push_field(
                            &value.serialize_as_bytes_with_options(args.flag_sep.as_bytes()),
                        );
                    }

                    writer.write_byte_record(&output_record)?;
                }

                Ok(())
            })?;
    } else {
        let mut record = csv::ByteRecord::new();
        let mut index: usize = 0;

        while reader.read_byte_record(&mut record)? {
            let cell = &record[column_index];

            let output_rows = {
                if cell.trim().is_empty() {
                    vec![vec![DynamicValue::None; padding]]
                } else {
                    let target = if let Some(input_dir) = &args.flag_input_dir {
                        ScraperTarget::HtmlFile(
                            input_dir,
                            std::str::from_utf8(cell).expect("invalid utf-8"),
                        )
                    } else {
                        ScraperTarget::HtmlCell(cell)
                    };

                    scraper.scrape(index, &record, target)?
                }
            };

            for output_row in output_rows {
                let mut output_record = if let Some(keep_sel) = &keep {
                    keep_sel.select(&record).collect()
                } else {
                    record.clone()
                };

                for value in output_row {
                    output_record.push_field(
                        &value.serialize_as_bytes_with_options(args.flag_sep.as_bytes()),
                    );
                }

                writer.write_byte_record(&output_record)?;
            }

            index += 1;
        }
    }

    Ok(writer.flush()?)
}
