use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::num::NonZeroUsize;

use rust_xlsxwriter::Workbook;

use crate::config::Config;
use crate::json::{JSONEmptyMode, JSONTypeInferrenceBuffer, OmittableAttributes};
use crate::util;
use crate::xml::XMLWriter;
use crate::CliResult;

static USAGE: &str = "
Convert a CSV file to a variety of data formats.

Usage:
    xan to <format> [options] [<input>]
    xan to --help

Supported formats:
    html    - HTML table
    json    - JSON array or object
    ndjson  - Newline-delimited JSON
    jsonl   - Newline-delimited JSON
    xlsx    - Excel spreasheet

JSON options:
    -B, --buffer-size <size>  Number of CSV rows to sample to infer column types.
                              [default: 512]
    --nulls                   Convert empty string to a null value.
    --omit                    Ignore the empty values.

Common options:
    -h, --help             Display this message
    -o, --output <file>    Write output to <file> instead of stdout.
";

#[derive(Deserialize)]
struct Args {
    arg_format: String,
    arg_input: Option<String>,
    flag_output: Option<String>,
    flag_buffer_size: NonZeroUsize,
    flag_nulls: bool,
    flag_omit: bool,
}

impl Args {
    fn is_writing_to_file(&self) -> bool {
        self.flag_output.is_some() || !io::stdout().is_terminal()
    }

    fn json_empty_mode(&self) -> JSONEmptyMode {
        if self.flag_nulls {
            JSONEmptyMode::Null
        } else if self.flag_omit {
            JSONEmptyMode::Omit
        } else {
            JSONEmptyMode::Empty
        }
    }

    fn convert_to_json<R: Read, W: Write>(
        &self,
        mut rdr: csv::Reader<R>,
        mut writer: W,
    ) -> CliResult<()> {
        let headers = rdr.headers()?.clone();

        let mut inferrence_buffer = JSONTypeInferrenceBuffer::with_columns(
            headers.len(),
            self.flag_buffer_size.get(),
            self.json_empty_mode(),
        );

        inferrence_buffer.read(&mut rdr)?;

        let mut json_object = OmittableAttributes::from_headers(headers.iter());
        let mut json_array = Vec::new();

        for record in inferrence_buffer.records() {
            inferrence_buffer.mutate_attributes(&mut json_object, record);
            json_array.push(json_object.clone());
        }

        let mut record = csv::StringRecord::new();

        while rdr.read_record(&mut record)? {
            inferrence_buffer.mutate_attributes(&mut json_object, &record);
            json_array.push(json_object.clone());
        }

        serde_json::to_writer_pretty(&mut writer, &json_array)?;
        writeln!(&mut writer)?;

        Ok(())
    }

    fn convert_to_ndjson<R: Read, W: Write>(
        &self,
        mut rdr: csv::Reader<R>,
        mut writer: W,
    ) -> CliResult<()> {
        let headers = rdr.headers()?.clone();
        let mut inferrence_buffer = JSONTypeInferrenceBuffer::with_columns(
            headers.len(),
            self.flag_buffer_size.get(),
            self.json_empty_mode(),
        );

        inferrence_buffer.read(&mut rdr)?;

        let mut json_object = OmittableAttributes::from_headers(headers.iter());

        for record in inferrence_buffer.records() {
            inferrence_buffer.mutate_attributes(&mut json_object, record);
            writeln!(writer, "{}", serde_json::to_string(&json_object)?)?;
        }

        let mut record = csv::StringRecord::new();

        while rdr.read_record(&mut record)? {
            inferrence_buffer.mutate_attributes(&mut json_object, &record);
            writeln!(writer, "{}", serde_json::to_string(&json_object)?)?;
        }

        Ok(())
    }

    fn convert_to_xlsx<R: Read>(
        &self,
        mut rdr: csv::Reader<R>,
        mut writer: Box<dyn Write>,
    ) -> CliResult<()> {
        if !self.is_writing_to_file() {
            Err("cannot export in xlsx without a path.\nUse -o, --output or pipe the result!")?;
        }

        let mut workbook = Workbook::new();
        let headers = rdr.headers()?.clone();
        let worksheet = workbook.add_worksheet();

        for (col, header) in headers.iter().enumerate() {
            worksheet.write_string(0, col as u16, header)?;
        }

        for (row, value) in rdr.records().enumerate() {
            let record = value?;
            for (col, field) in record.iter().enumerate() {
                worksheet.write_string((row + 1) as u32, col as u16, field)?;
            }
        }

        let mut cursor = io::Cursor::new(Vec::new());
        workbook.save_to_writer(&mut cursor)?;
        let buf = cursor.into_inner();
        writer.write_all(&buf)?;

        writer.flush()?;
        Ok(())
    }

    fn convert_to_html<R: Read>(
        &self,
        mut rdr: csv::Reader<R>,
        writer: Box<dyn Write>,
    ) -> CliResult<()> {
        let mut xml_writer = XMLWriter::new(writer);
        let mut record = csv::StringRecord::new();

        xml_writer.open_no_attributes("table")?;
        xml_writer.open_no_attributes("thead")?;
        xml_writer.open_no_attributes("tr")?;

        for header in rdr.headers()?.iter() {
            xml_writer.open_no_attributes("th")?;
            xml_writer.write_text(header)?;
            xml_writer.close("th")?;
        }

        xml_writer.close("tr")?;
        xml_writer.close("thead")?;

        xml_writer.open_no_attributes("tbody")?;

        while rdr.read_record(&mut record)? {
            xml_writer.open_no_attributes("tr")?;

            for cell in record.iter() {
                xml_writer.open_no_attributes("td")?;
                xml_writer.write_text(cell)?;
                xml_writer.close("td")?;
            }

            xml_writer.close("tr")?;
        }

        xml_writer.close("tbody")?;

        xml_writer.close("table")?;
        xml_writer.finish()?;

        Ok(())
    }
}

pub fn run(argv: &[&str]) -> CliResult<()> {
    let args: Args = util::get_args(USAGE, argv)?;
    let conf = Config::new(&args.arg_input);
    let rdr = conf.reader()?;

    let writer: Box<dyn Write> = match &args.flag_output {
        Some(output_path) => Box::new(fs::File::create(output_path)?),
        None => Box::new(io::stdout()),
    };

    match args.arg_format.as_str() {
        "html" => args.convert_to_html(rdr, writer)?,
        "json" => args.convert_to_json(rdr, writer)?,
        "jsonl" | "ndjson" => args.convert_to_ndjson(rdr, writer)?,
        "xlsx" => args.convert_to_xlsx(rdr, writer)?,
        _ => Err("could not export the file to this format!")?,
    }

    Ok(())
}
