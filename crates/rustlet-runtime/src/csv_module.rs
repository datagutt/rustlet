use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::list::ListRef;
use starlark::values::Value;

#[starlark::starlark_module]
pub fn csv_module(builder: &mut GlobalsBuilder) {
    fn read_all<'v>(
        s: &str,
        #[starlark(default = ",")] comma: &str,
        #[starlark(default = "")] comment: &str,
        #[starlark(default = false)] lazy_quotes: bool,
        #[starlark(default = false)] trim_leading_space: bool,
        #[starlark(default = 0)] fields_per_record: i32,
        #[starlark(default = 0)] skip: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        if skip < 0 {
            return Err(anyhow::anyhow!("skip cannot be negative"));
        }

        let mut builder = ::csv::ReaderBuilder::new();
        // Pixlet trims only leading whitespace; rust's csv `Trim::Fields` trims both sides,
        // so we keep the reader as `Trim::None` and post-process fields below.
        builder
            .has_headers(false)
            .delimiter(csv_delimiter(comma, "comma")?)
            .flexible(lazy_quotes || fields_per_record <= 0)
            .trim(::csv::Trim::None);
        if !comment.is_empty() {
            builder.comment(Some(csv_delimiter(comment, "comment")?));
        }

        let mut reader = builder.from_reader(s.as_bytes());
        let mut rows = Vec::new();
        for (index, record) in reader.records().enumerate() {
            let record = record.map_err(|e| anyhow::anyhow!("CSV parse error: {e}"))?;
            if index < skip as usize {
                continue;
            }
            if fields_per_record > 0 && record.len() != fields_per_record as usize {
                return Err(anyhow::anyhow!(
                    "wrong number of fields in record: expected {}, got {}",
                    fields_per_record,
                    record.len()
                ));
            }
            rows.push(
                record
                    .iter()
                    .map(|field| {
                        let f = if trim_leading_space {
                            field.trim_start()
                        } else {
                            field
                        };
                        eval.heap().alloc(f)
                    })
                    .collect::<Vec<_>>(),
            );
        }

        Ok(eval.heap().alloc(rows))
    }

    fn write_all(rows: Value, #[starlark(default = ",")] comma: &str) -> anyhow::Result<String> {
        let rows = ListRef::from_value(rows)
            .ok_or_else(|| anyhow::anyhow!("csv.write_all expects a list of rows"))?;
        let mut writer = ::csv::WriterBuilder::new()
            .has_headers(false)
            .delimiter(csv_delimiter(comma, "comma")?)
            .from_writer(Vec::new());

        for row in rows.iter() {
            let fields = ListRef::from_value(row)
                .ok_or_else(|| anyhow::anyhow!("csv.write_all rows must be lists"))?;
            let mut record = Vec::with_capacity(fields.len());
            for field in fields.iter() {
                let field = field
                    .unpack_str()
                    .ok_or_else(|| anyhow::anyhow!("csv.write_all fields must be strings"))?;
                record.push(field);
            }
            writer
                .write_record(record)
                .map_err(|e| anyhow::anyhow!("CSV write error: {e}"))?;
        }

        let bytes = writer
            .into_inner()
            .map_err(|e| anyhow::anyhow!("CSV write error: {e}"))?;
        String::from_utf8(bytes).map_err(|e| anyhow::anyhow!("CSV output is not valid UTF-8: {e}"))
    }
}

pub fn build_csv_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(csv_module)
        .build()
}

fn csv_delimiter(value: &str, name: &str) -> anyhow::Result<u8> {
    let bytes = value.as_bytes();
    if bytes.len() != 1 {
        return Err(anyhow::anyhow!(
            "{name} must be a single-byte character"
        ));
    }
    let byte = bytes[0];
    if byte == b'\n' || byte == b'\r' {
        return Err(anyhow::anyhow!(
            "{name} must be a non-newline character"
        ));
    }
    Ok(byte)
}
