<!-- Generated -->
# xan groupby

```txt
Group a CSV file by values contained in a column selection then aggregate data per
group using a custom aggregation expression.

The result of running the command will be a CSV file containing the grouped
columns and additional columns for each computed aggregation.

You can, for instance, compute the sum of a column per group:

    $ xan groupby user_name 'sum(retweet_count)' file.csv

You can use dynamic expressions to mangle the data before aggregating it:

    $ xan groupby user_name 'sum(retweet_count + replies_count)' file.csv

You can perform multiple aggregations at once:

    $ xan groupby user_name 'sum(retweet_count), mean(retweet_count), max(replies_count)' file.csv

You can rename the output columns using the 'as' syntax:

    $ xan groupby user_name 'sum(n) as sum, max(replies_count) as "Max Replies"' file.csv

You can group on multiple columns (read `xan select -h` for more information about column selection):

    $ xan groupby name,surname 'sum(count)' file.csv

For a list of available aggregation functions, use `xan help aggs`
instead.

For a quick review of the capabilities of the expression language,
check out the `xan help cheatsheet` command.

For a list of available functions, use `xan help functions`.

Aggregations can be computed in parallel using the -p/--parallel or -t/--threads flags.
But this cannot work on streams or gzipped files, unless a `.gzi` index (as created
by `bgzip -i`) can be found beside it. Parallelization is not compatible
with the -S/--sorted nor -E/--errors options.

Note that when using parallelization, groups may appear in different order in the
output with each run.

Usage:
    xan groupby [options] <column> <expression> [<input>]
    xan groupby --help

groupby options:
    --keep <cols>            Keep this selection of columns, in addition to
                             the ones representing groups, in the output. Only
                             values from the first seen row per group will be kept.
    -S, --sorted             Use this flag to indicate that the file is already sorted on the
                             group columns, in which case the command will be able to considerably
                             optimize memory usage.
    -e, --errors <policy>    What to do with evaluation errors. One of:
                               - "panic": exit on first error
                               - "ignore": ignore row altogether
                               - "log": print error to stderr
                             [default: panic].
    -p, --parallel           Whether to use parallelization to speed up computation.
                             Will automatically select a suitable number of threads to use
                             based on your number of cores. Use -t, --threads if you want to
                             indicate the number of threads yourself.
    -t, --threads <threads>  Parellize computations using this many threads. Use -p, --parallel
                             if you want the number of threads to be automatically chosen instead.

Common options:
    -h, --help               Display this message
    -o, --output <file>      Write output to <file> instead of stdout.
    -n, --no-headers         When set, the first row will not be evaled
                             as headers.
    -d, --delimiter <arg>    The field delimiter for reading CSV data.
                             Must be a single character.
```
