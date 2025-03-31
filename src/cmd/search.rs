use std::borrow::Cow;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::str::from_utf8;

use aho_corasick::AhoCorasick;
use bstr::ByteSlice;
use regex::bytes::{Regex, RegexBuilder};
use regex_automata::{meta::Regex as LowLevelRegex, util::syntax};

use crate::config::{Config, Delimiter};
use crate::select::SelectColumns;
use crate::urls::{LRUStems, LRUTrie, TaggedUrl};
use crate::util;
use crate::CliError;
use crate::CliResult;

fn count_overlapping_matches(regex: &Regex, haystack: &[u8]) -> usize {
    let mut count: usize = 0;
    let mut offset: usize = 0;

    while let Some(m) = regex.find_at(haystack, offset) {
        count += 1;

        if m.start() == offset {
            offset += 1;
        } else {
            offset = m.end();
        }
    }

    count
}

enum Matcher {
    Empty,
    NonEmpty,
    Substring(AhoCorasick, bool),
    Exact(Vec<u8>, bool),
    Regex(Regex),
    Regexes(Vec<Regex>),
    RegexSet(LowLevelRegex),
    HashSet(HashSet<Vec<u8>>, bool),
    UrlPrefix(LRUStems),
    UrlTrie(LRUTrie),
}

impl Matcher {
    fn is_match(&self, cell: &[u8]) -> bool {
        match self {
            Self::Empty => cell.is_empty(),
            Self::NonEmpty => !cell.is_empty(),
            Self::Substring(pattern, case_insensitive) => {
                if *case_insensitive {
                    pattern.is_match(&cell.to_lowercase())
                } else {
                    pattern.is_match(cell)
                }
            }
            Self::Regex(pattern) => pattern.is_match(cell),
            Self::Regexes(_) => unreachable!(),
            Self::Exact(pattern, case_insensitive) => {
                if *case_insensitive {
                    &cell.to_lowercase() == pattern
                } else {
                    cell == pattern
                }
            }
            Self::RegexSet(set) => set.is_match(cell),
            Self::HashSet(patterns, case_insensitive) => {
                if *case_insensitive {
                    patterns.contains(&cell.to_lowercase())
                } else {
                    patterns.contains(cell)
                }
            }
            Self::UrlPrefix(stems) => match from_utf8(cell).ok() {
                None => false,
                Some(url) => stems.is_simplified_match(url),
            },
            Self::UrlTrie(trie) => match from_utf8(cell).ok() {
                None => false,
                Some(url) => trie.is_match(url).unwrap_or(false),
            },
        }
    }

    fn count(&self, cell: &[u8], overlapping: bool) -> usize {
        match self {
            Self::Empty => {
                if cell.is_empty() {
                    1
                } else {
                    0
                }
            }
            Self::NonEmpty => {
                if cell.is_empty() {
                    0
                } else {
                    1
                }
            }
            Self::Substring(pattern, case_insensitive) => match (*case_insensitive, overlapping) {
                (true, false) => pattern.find_iter(&cell.to_lowercase()).count(),
                (false, false) => pattern.find_iter(cell).count(),
                (true, true) => pattern.find_overlapping_iter(&cell.to_lowercase()).count(),
                (false, true) => pattern.find_overlapping_iter(cell).count(),
            },
            Self::Regex(pattern) => {
                if !overlapping {
                    pattern.find_iter(cell).count()
                } else {
                    count_overlapping_matches(pattern, cell)
                }
            }
            Self::Exact(pattern, case_insensitive) => {
                if *case_insensitive {
                    if &cell.to_lowercase() == pattern {
                        1
                    } else {
                        0
                    }
                } else if cell == pattern {
                    1
                } else {
                    0
                }
            }
            Self::RegexSet(set) => {
                if overlapping {
                    unreachable!()
                }
                set.find_iter(cell).count()
            }
            Self::Regexes(patterns) => patterns
                .iter()
                .map(|pattern| count_overlapping_matches(pattern, cell))
                .sum(),
            Self::HashSet(patterns, case_insensitive) => {
                if *case_insensitive {
                    if patterns.contains(&cell.to_lowercase()) {
                        1
                    } else {
                        0
                    }
                } else if patterns.contains(cell) {
                    1
                } else {
                    0
                }
            }
            Self::UrlPrefix(stems) => match from_utf8(cell).ok() {
                None => 0,
                Some(url) => {
                    if stems.is_simplified_match(url) {
                        1
                    } else {
                        0
                    }
                }
            },
            Self::UrlTrie(trie) => match from_utf8(cell).ok() {
                None => 0,
                Some(url) => {
                    if trie.is_match(url).unwrap_or(false) {
                        1
                    } else {
                        0
                    }
                }
            },
        }
    }

    fn replace<'a>(&self, cell: &'a [u8], with: &'a [u8]) -> Cow<'a, [u8]> {
        match self {
            Self::Empty => {
                if cell.is_empty() {
                    Cow::Borrowed(with)
                } else {
                    Cow::Borrowed(cell)
                }
            }
            Self::NonEmpty => {
                if cell.is_empty() {
                    Cow::Borrowed(cell)
                } else {
                    Cow::Borrowed(with)
                }
            }
            Self::Substring(pattern, case_insensitive) => {
                if *case_insensitive {
                    Cow::Owned(pattern.replace_all_bytes(&cell.to_lowercase(), &[with]))
                } else {
                    Cow::Owned(pattern.replace_all_bytes(cell, &[with]))
                }
            }
            Self::Regex(pattern) => pattern.replace_all(cell, with),
            Self::Exact(pattern, case_insensitive) => {
                if *case_insensitive {
                    if &cell.to_lowercase() == pattern {
                        Cow::Borrowed(with)
                    } else {
                        Cow::Borrowed(cell)
                    }
                } else if cell == pattern {
                    Cow::Borrowed(with)
                } else {
                    Cow::Borrowed(cell)
                }
            }
            Self::RegexSet(_) => unreachable!(),
            Self::Regexes(_) => unreachable!(),
            Self::HashSet(patterns, case_insensitive) => {
                if *case_insensitive {
                    if patterns.contains(&cell.to_lowercase()) {
                        Cow::Borrowed(with)
                    } else {
                        Cow::Borrowed(cell)
                    }
                } else if patterns.contains(cell) {
                    Cow::Borrowed(with)
                } else {
                    Cow::Borrowed(cell)
                }
            }
            Self::UrlPrefix(stems) => match from_utf8(cell).ok() {
                None => Cow::Borrowed(cell),
                Some(url) => {
                    if stems.is_simplified_match(url) {
                        Cow::Borrowed(with)
                    } else {
                        Cow::Borrowed(cell)
                    }
                }
            },
            Self::UrlTrie(trie) => match from_utf8(cell).ok() {
                None => Cow::Borrowed(cell),
                Some(url) => {
                    if trie.is_match(url).unwrap_or(false) {
                        Cow::Borrowed(with)
                    } else {
                        Cow::Borrowed(cell)
                    }
                }
            },
        }
    }
}

// NOTE: a -U, --unbuffered flag that flushes on each match does not solve
// early termination when piping to `xan slice` because flush won't get a broken
// pipe when writing nothing.
static USAGE: &str = "
Keep rows of given CSV file if ANY of the selected columns contains a desired
substring.

Can also be used to search for exact matches using the -e, --exact flag.

Can also be used to search using a regular expression using the -r, --regex flag.

Can also be used to search by url prefix (e.g. \"lemonde.fr/business\") using
the -u, --url-prefix flag.

Can also be used to search for empty or non-empty selections. For instance,
keeping only rows where selection is not fully empty:

    $ xan search --non-empty file.csv

Or keeping only rows where selection has any empty column:

    $ xan search --empty file.csv

When using a regular expression, be sure to mind bash escape rules (prefer single
quotes around your expression and don't forget to use backslashes when needed):

    $ xan search -r '\\bfran[cç]' file.csv

To restrict the columns that will be searched you can use the -s, --select flag.

All search modes can also be case-insensitive using -i, --ignore-case.

Finally, this command is also able to search for multiple patterns at once.
To do so, you must give a text file with one pattern per line to the --patterns
flag, or a CSV file containing a column of to indicate using --pattern-column.

One pattern per line of text file:

    $ xan search --patterns patterns.txt file.csv > matches.csv

CSV column containing patterns:

    $ xan search --patterns people.csv --pattern-column name tweets.csv > matches.csv

Feeding patterns through stdin (using \"-\"):

    $ cat patterns.txt | xan search --patterns - file.csv > matches.csv

Feeding CSV column as patterns through stdin (using \"-\"):

    $ xan slice -l 10 people.csv | xan search --patterns - --pattern-column name file.csv > matches.csv

Usage:
    xan search [options] --non-empty [<input>]
    xan search [options] --empty [<input>]
    xan search [options] --patterns <index> [<input>]
    xan search [options] <pattern> [<input>]
    xan search --help

search options:
    -e, --exact              Perform an exact match.
    -r, --regex              Use a regex to perform the match.
    -E, --empty              Search for empty cells, i.e. filter out
                             any completely non-empty selection.
    -N, --non-empty          Search for non-empty cells, i.e. filter out
                             any completely empty selection.
    -u, --url-prefix         Match by url prefix, i.e. cells must contain urls
                             matching the searched url prefix. Urls are first
                             reordered using a scheme called a LRU, that you can
                             read about here:
                             https://github.com/medialab/ural?tab=readme-ov-file#about-lrus
    --patterns <path>        Path to a text file (use \"-\" for stdin), containing multiple
                             patterns, one per line, to search at once.
    --pattern-column <name>  When given a column name, --patterns file will be considered a CSV
                             and patterns to search will be extracted from the given column.
    -i, --ignore-case        Case insensitive search.
    -s, --select <arg>       Select the columns to search. See 'xan select -h'
                             for the full syntax.
    -v, --invert-match       Select only rows that did not match
    -A, --all                Only return a row when ALL columns from the given selection
                             match the desired pattern, instead of returning a row
                             when ANY column matches.
    -c, --count <column>     If given, the command will not filter rows but will instead
                             count the total number of non-overlapping pattern matches per
                             row and report it in a new column with given name.
                             Does not work with -v/--invert-match.
    -R, --replace <with>     If given, the command will not filter rows but will instead
                             replace matches with the given replacement.
    --overlapping            When used with -c/--count, return the count of overlapping
                             matches. Note that this can sometimes be one order of magnitude
                             slower that counting non-overlapping matches.
    -l, --limit <n>          Maximum of number rows to return. Useful to avoid downstream
                             buffering some times (e.g. when searching for very few
                             rows in a big file before piping to `view` or `flatten`).

Common options:
    -h, --help             Display this message
    -o, --output <file>    Write output to <file> instead of stdout.
    -n, --no-headers       When set, the first row will not be interpreted
                           as headers. (i.e., They are not searched, analyzed,
                           sliced, etc.)
    -d, --delimiter <arg>  The field delimiter for reading CSV data.
                           Must be a single character.
";

#[derive(Deserialize)]
struct Args {
    arg_input: Option<String>,
    arg_pattern: Option<String>,
    flag_select: SelectColumns,
    flag_output: Option<String>,
    flag_no_headers: bool,
    flag_delimiter: Option<Delimiter>,
    flag_invert_match: bool,
    flag_overlapping: bool,
    flag_all: bool,
    flag_ignore_case: bool,
    flag_empty: bool,
    flag_non_empty: bool,
    flag_exact: bool,
    flag_regex: bool,
    flag_url_prefix: bool,
    flag_count: Option<String>,
    flag_replace: Option<String>,
    flag_limit: Option<NonZeroUsize>,
    flag_patterns: Option<String>,
    flag_pattern_column: Option<SelectColumns>,
}

impl Args {
    fn build_matcher(&self) -> Result<Matcher, CliError> {
        if self.flag_non_empty {
            return Ok(Matcher::NonEmpty);
        }

        if self.flag_empty {
            return Ok(Matcher::Empty);
        }

        match self.flag_patterns.as_ref() {
            None => {
                let pattern = self.arg_pattern.as_ref().unwrap();

                Ok(if self.flag_exact {
                    if self.flag_ignore_case {
                        Matcher::Exact(pattern.as_bytes().to_lowercase(), true)
                    } else {
                        Matcher::Exact(pattern.as_bytes().to_vec(), false)
                    }
                } else if self.flag_regex {
                    Matcher::Regex(
                        RegexBuilder::new(pattern)
                            .case_insensitive(self.flag_ignore_case)
                            .build()?,
                    )
                } else if self.flag_url_prefix {
                    let tagged_url = pattern.parse::<TaggedUrl>()?;

                    Matcher::UrlPrefix(LRUStems::from_tagged_url(&tagged_url, true))
                } else {
                    Matcher::Substring(
                        AhoCorasick::new([if self.flag_ignore_case {
                            pattern.to_lowercase()
                        } else {
                            pattern.to_string()
                        }])?,
                        self.flag_ignore_case,
                    )
                })
            }
            Some(_) => {
                let patterns = Config::new(&self.flag_patterns)
                    .delimiter(self.flag_delimiter)
                    .lines(&self.flag_pattern_column)?;

                Ok(if self.flag_exact {
                    Matcher::HashSet(
                        patterns
                            .map(|pattern| {
                                pattern.map(|p| {
                                    if self.flag_ignore_case {
                                        p.to_lowercase().into_bytes()
                                    } else {
                                        p.into_bytes()
                                    }
                                })
                            })
                            .collect::<Result<HashSet<_>, _>>()?,
                        self.flag_ignore_case,
                    )
                } else if self.flag_regex {
                    if self.flag_overlapping {
                        Matcher::Regexes(
                            patterns
                                .map(|pattern| {
                                    pattern.and_then(|p| {
                                        RegexBuilder::new(&p)
                                            .case_insensitive(self.flag_ignore_case)
                                            .build()
                                            .map_err(CliError::from)
                                    })
                                })
                                .collect::<Result<Vec<_>, _>>()?,
                        )
                    } else {
                        Matcher::RegexSet(
                            LowLevelRegex::builder()
                                .syntax(
                                    syntax::Config::new().case_insensitive(self.flag_ignore_case),
                                )
                                .build_many(&patterns.collect::<Result<Vec<_>, _>>()?)?,
                        )
                    }
                } else if self.flag_url_prefix {
                    let mut trie = LRUTrie::new_simplified();

                    for result in patterns {
                        let url = result?;
                        trie.add(&url)?;
                    }

                    Matcher::UrlTrie(trie)
                } else {
                    Matcher::Substring(
                        AhoCorasick::new(
                            &patterns
                                .map(|pattern| {
                                    pattern.map(|p| {
                                        if self.flag_ignore_case {
                                            p.to_lowercase()
                                        } else {
                                            p
                                        }
                                    })
                                })
                                .collect::<Result<Vec<_>, _>>()?,
                        )?,
                        self.flag_ignore_case,
                    )
                })
            }
        }
    }
}

pub fn run(argv: &[&str]) -> CliResult<()> {
    let args: Args = util::get_args(USAGE, argv)?;

    let matchers_count: u8 = args.flag_exact as u8
        + args.flag_regex as u8
        + args.flag_non_empty as u8
        + args.flag_empty as u8
        + args.flag_url_prefix as u8;

    if matchers_count > 1 {
        Err("must select only one of -e/--exact, -N/--non-empty, -E/--empty, -u/--url-prefix or -r/--regex!")?;
    }

    if args.flag_overlapping && args.flag_count.is_none() {
        Err("--overlapping only works with -c/--count!")?;
    }

    if args.flag_count.is_some() || args.flag_replace.is_some() {
        if args.flag_invert_match {
            Err("-c/--count & -R/--replace do not work with -v/--invert-match!")?;
        }

        if args.flag_all {
            Err("-c/--count & -R/--replace do not work with -A/--all!")?;
        }
    }

    if (args.flag_empty || args.flag_non_empty) && args.flag_patterns.is_some() {
        Err("-N/--non-empty & -E/--empty do not make sense with --patterns!")?;
    }

    if args.flag_ignore_case && args.flag_url_prefix {
        Err("-u/--url-prefix & -i/--ignore-case are not compatible!")?;
    }

    if args.flag_count.is_some() && args.flag_replace.is_some() {
        Err("-c/--count does not work with -R/--replace!")?;
    }

    if args.flag_patterns.is_some() && args.flag_replace.is_some() {
        unimplemented!()
    }

    let matcher = args.build_matcher()?;
    let rconfig = Config::new(&args.arg_input)
        .delimiter(args.flag_delimiter)
        .no_headers(args.flag_no_headers)
        .select(args.flag_select);

    let mut rdr = rconfig.reader()?;
    let mut wtr = Config::new(&args.flag_output).writer()?;

    let mut headers = rdr.byte_headers()?.clone();
    let sel = rconfig.selection(&headers)?;

    if let Some(column_name) = &args.flag_count {
        headers.push_field(column_name.as_bytes());
    }

    if !rconfig.no_headers {
        wtr.write_record(&headers)?;
    }

    let mut record = csv::ByteRecord::new();
    let mut replaced_record = csv::ByteRecord::new();
    let mut i: usize = 0;

    while rdr.read_byte_record(&mut record)? {
        let mut is_match: bool = false;

        if let Some(replacement) = &args.flag_replace {
            replaced_record.clear();

            for cell in sel.select(&record) {
                let replaced_cell = matcher.replace(cell, replacement.as_bytes());
                replaced_record.push_field(&replaced_cell);

                if args.flag_limit.is_some() && cell != replaced_cell.as_ref() {
                    is_match = true;
                }
            }

            wtr.write_byte_record(&replaced_record)?;
        } else if args.flag_count.is_some() {
            let count: usize = sel
                .select(&record)
                .map(|cell| matcher.count(cell, args.flag_overlapping))
                .sum();

            if count > 0 {
                is_match = true;
            }

            record.push_field(count.to_string().as_bytes());
            wtr.write_byte_record(&record)?;
        } else {
            is_match = if args.flag_all {
                sel.select(&record).all(|cell| matcher.is_match(cell))
            } else {
                sel.select(&record).any(|cell| matcher.is_match(cell))
            };

            if args.flag_invert_match {
                is_match = !is_match;
            }

            if is_match {
                wtr.write_byte_record(&record)?;
            }
        }

        if let Some(limit) = args.flag_limit {
            if is_match {
                i += 1;
            }

            if i >= limit.get() {
                break;
            }
        }
    }

    Ok(wtr.flush()?)
}
