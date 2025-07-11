<!-- Generated -->
# xan select

```txt
Select columns from CSV data using a shorthand notation or by evaluating an expression
on each row (using the -e/--evaluate or -f/--evaluate-file flag).

This command lets you manipulate columns of CSV data. You can re-order
them, duplicate them, transform them or even drop them in the process.

# Shorthand notation

Columns can be referenced using a zero-based index, or a negative index starting
from the end, or a name (if the file has headers) or a name and nth, so you
can easily select columns with duplicate names.

You can also select columns by prefix or suffix using `*` in the column name.

Finally, it is also possible to select ranges of columns using the `:`
character. Note that column ranges are always inclusive.

Examples:

  Select the first and fourth columns:
    $ xan select 0,3

  Select the last column using negative indexing (notice the `--`
  to avoid shell issues with arguments starting with hyphens):
    $ xan select -- -1

  Select first and next to last:
    $ xan select 0,-2

  Select the first 4 columns (by index or by name):
    $ xan select 0:3
    $ xan select Header1:Header4

  Ignore the first 2 columns (by range and by omission):
    $ xan select 2:
    $ xan select '!0:1' (use single quotes to avoid shell issues!)

  Select using negative indices in range:
    $ xan select 3:-2 (fourth to next to last)
    $ xan select -- -3: (last three columns)
    $ xan select :-3 (up to the third from last)

  Select the third column named 'Foo':
    $ xan select 'Foo[2]'

  Select the last column named 'Foo':
    $ xan select 'Foo[-1]'

  Select column names containing spaces:
    $ xan select "Revenues in millions"
    $ xan select Revenues\ in\ millions
    $ xan select 1,"Revenues in millions",year

  Re-order and duplicate columns arbitrarily:
    $ xan select 3:1,Header3:Header1,Header1,Foo[2],Header1

  Quote column names that conflict with selector syntax,
  (mind the double quoting, problematic characters being `*`, `:`, `!`, `[` and `]`):
    $ xan select '"Start:datetime","Count:int"'

  Select all the columns which is useful to add some copies of columns
  (notice the simple quotes to avoid shell globbing):
    $ xan select '*'
    $ xan select '*,name'
    $ xan select '*,1'
    $ xan select '0:'
    $ xan select ':0'

  Select all columns starting by "dim_"
  (notice the simple quotes again):
    $ xan select 'dim_*'

  Select all columns ending by "_count":
    $ xan select '*_count'

# Evaluating a expression

Using a SQLish syntax that is the same as for the `map`, `agg`, `filter` etc.
commands, you can wrangle the rows and perform a custom selection.

  $ xan select -e 'id, name as surname, count1 + count2 as total'

If your expression becomes too complicated, you can write it in a file and
use the -f/--evaluate-file flag instead:

  $ xan select -f selection.moonblade

For a quick review of the capabilities of the expression language,
check out the `xan help cheatsheet` command.

For a list of available functions, use `xan help functions`.

Usage:
    xan select [options] [--] <selection> [<input>]
    xan select --help

select options:
    -e, --evaluate       Toggle expression evaluation rather than using the
                         shorthand selection notation.
    -f, --evaluate-file  Read evaluation expression from a file instead.

Common options:
    -h, --help             Display this message
    -o, --output <file>    Write output to <file> instead of stdout.
    -n, --no-headers       When set, the first row will not be interpreted
                           as headers. (i.e., They are not searched, analyzed,
                           sliced, etc.)
    -d, --delimiter <arg>  The field delimiter for reading CSV data.
                           Must be a single character.
```
