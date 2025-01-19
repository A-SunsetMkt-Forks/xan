use std::env;
use std::fs::File;
use std::path::PathBuf;

use glob::glob;

static COMMANDS: [&str; 57] = [
    "agg",
    "behead",
    "bins",
    "blank",
    "cat",
    "cluster",
    "count",
    "dedup",
    "enum",
    "eval",
    "explode",
    "foreach",
    "fill",
    "filter",
    "fixlengths",
    "flatmap",
    "flatten",
    "fmt",
    "frequency",
    "from",
    "glob",
    "groupby",
    "guillotine",
    "headers",
    "help",
    "heatmap",
    "hist",
    "implode",
    "index",
    "input",
    "join",
    "map",
    "matrix",
    "merge",
    "network",
    "parallel",
    "partition",
    "plot",
    "progress",
    "range",
    "rename",
    "reverse",
    "sample",
    "search",
    "select",
    "shuffle",
    "slice",
    "sort",
    "split",
    "stats",
    "tokenize",
    "top",
    "transform",
    "transpose",
    "union-find",
    "view",
    "vocab",
];

static MATRIX_SUBCOMMANDS: [&str; 1] = ["corr"];
static NETWORK_SUBCOMMANDS: [&str; 2] = ["edgelist", "bipartite"];
static TOKENIZE_SUBCOMMANDS: [&str; 3] = ["words", "sentences", "paragraphs"];
static VOCAB_SUBCOMMANDS: [&str; 5] = ["corpus", "doc", "doc-token", "token", "cooc"];

fn find_csv_files_in_prompt() -> Vec<String> {
    let words = shlex::split(&env::var("COMP_LINE").unwrap_or("".to_string())).unwrap_or_default();

    words
        .into_iter()
        .filter(|p| {
            p.ends_with(".csv")
                || p.ends_with(".tsv")
                || p.ends_with(".csv.gz")
                || p.ends_with(".tsv.gz")
        })
        .take(15)
        .collect()
}

fn most_likely_csv_files_by_glob() -> impl Iterator<Item = PathBuf> {
    glob("*.csv")
        .unwrap()
        .chain(glob("*.csv.gz").unwrap())
        .chain(glob("*.tsv").unwrap())
        .chain(glob("*.tsv.gz").unwrap())
        .chain(glob("*/*.csv").unwrap())
        .chain(glob("*/*.csv.gz").unwrap())
        .chain(glob("*/*.tsv").unwrap())
        .chain(glob("*/*.tsv.gz").unwrap())
        .take(15)
        .map(|p| p.unwrap())
}

fn find_csv_files_to_test() -> Vec<PathBuf> {
    let in_prompt = find_csv_files_in_prompt();

    if !in_prompt.is_empty() {
        return in_prompt.into_iter().map(PathBuf::from).collect();
    }

    most_likely_csv_files_by_glob().collect()
}

pub fn run() {
    let args = env::args().collect::<Vec<_>>();

    let mut to_complete = args[3].as_str();
    let word_before = &args[4];

    if to_complete == "--" {
        to_complete = "";
    }

    // Completing commands
    if word_before == "xan" {
        if !to_complete.starts_with('-') {
            for command in COMMANDS {
                if command.starts_with(to_complete) {
                    println!("{}", command);
                }
            }
        }
    } else if word_before == "matrix" && !to_complete.starts_with('-') {
        for subcommand in MATRIX_SUBCOMMANDS {
            if subcommand.starts_with(to_complete) {
                println!("{}", subcommand);
            }
        }
    } else if word_before == "network" && !to_complete.starts_with('-') {
        for subcommand in NETWORK_SUBCOMMANDS {
            if subcommand.starts_with(to_complete) {
                println!("{}", subcommand);
            }
        }
    } else if word_before == "vocab" && !to_complete.starts_with('-') {
        for subcommand in VOCAB_SUBCOMMANDS {
            if subcommand.starts_with(to_complete) {
                println!("{}", subcommand);
            }
        }
    } else if word_before == "tokenize" && !to_complete.starts_with('-') {
        for subcommand in TOKENIZE_SUBCOMMANDS {
            if subcommand.starts_with(to_complete) {
                println!("{}", subcommand);
            }
        }
    }
    // Completing column selectors
    else if word_before == "-s"
        || word_before == "--select"
        || word_before == "-g"
        || word_before == "--groupby"
        || word_before == "select"
        || word_before == "transform"
        || word_before == "explode"
        || word_before == "implode"
        || word_before == "groupby"
        || word_before == "partition"
        || word_before == "plot"
        || word_before == "top"
    {
        let mut all_headers = Vec::<String>::new();

        let to_complete = to_complete.trim_matches(|c: char| c == '\'' || c == '"');

        for path in find_csv_files_to_test() {
            let file = match File::open(path) {
                Ok(f) => f,
                _ => continue,
            };
            let mut reader = csv::Reader::from_reader(file);

            if let Ok(headers) = reader.headers() {
                for name in headers {
                    let name = name.to_string();

                    if name.starts_with(to_complete) && !all_headers.contains(&name) {
                        all_headers.push(name);
                    }
                }
            }
        }

        for name in all_headers {
            println!("{}", name);
        }
    }
}
