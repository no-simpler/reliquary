//! Column-aligned tables. Widths are measured rather than declared, so a count
//! wider than its heading cannot push one row out of line with the rest.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

pub struct Column {
    header: &'static str,
    align: Align,
}

impl Column {
    pub fn left(header: &'static str) -> Self {
        Column {
            header,
            align: Align::Left,
        }
    }

    pub fn right(header: &'static str) -> Self {
        Column {
            header,
            align: Align::Right,
        }
    }
}

pub struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(columns: Vec<Column>) -> Self {
        Table {
            columns,
            rows: Vec::new(),
        }
    }

    /// `depth` indents the first cell two spaces per level — what separates a
    /// roll-up from the rows it sums.
    pub fn push(&mut self, depth: usize, mut cells: Vec<String>) {
        if let Some(first) = cells.first_mut() {
            *first = format!("{}{first}", "  ".repeat(depth));
        }
        cells.resize(self.columns.len(), String::new());
        self.rows.push(cells);
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn render(&self) -> String {
        if self.rows.is_empty() {
            return String::new();
        }

        let widths: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, column)| {
                self.rows
                    .iter()
                    .filter_map(|row| row.get(i))
                    .map(|cell| cell.chars().count())
                    .chain(std::iter::once(column.header.chars().count()))
                    .max()
                    .unwrap_or(0)
            })
            .collect();

        let mut out = String::new();
        let headers: Vec<&str> = self.columns.iter().map(|c| c.header).collect();
        line(&mut out, &headers, &widths, &self.columns);
        for row in &self.rows {
            let cells: Vec<&str> = row.iter().map(String::as_str).collect();
            line(&mut out, &cells, &widths, &self.columns);
        }
        out
    }
}

/// One rendered line: a two-space gutter, two spaces between columns, and no
/// padding past the last cell that carries text.
fn line(out: &mut String, cells: &[&str], widths: &[usize], columns: &[Column]) {
    let mut rendered = String::from("  ");
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            rendered.push_str("  ");
        }
        let pad = widths
            .get(i)
            .copied()
            .unwrap_or(0)
            .saturating_sub(cell.chars().count());
        match columns.get(i).map_or(Align::Left, |column| column.align) {
            Align::Left => {
                rendered.push_str(cell);
                rendered.push_str(&" ".repeat(pad));
            }
            Align::Right => {
                rendered.push_str(&" ".repeat(pad));
                rendered.push_str(cell);
            }
        }
    }
    out.push_str(rendered.trim_end());
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(values: &[&str]) -> Vec<String> {
        values
            .iter()
            .map(std::string::ToString::to_string)
            .collect()
    }

    fn table() -> Table {
        let mut table = Table::new(vec![
            Column::left("name"),
            Column::right("count"),
            Column::left("note"),
        ]);
        table.push(0, cells(&["source", "7", "roll-up"]));
        table.push(1, cells(&["php", "1,533,873", "detail"]));
        table
    }

    #[test]
    fn columns_size_to_their_widest_cell() {
        let rendered = table().render();
        let lines: Vec<&str> = rendered.lines().collect();
        // "source" sets the first column at 6, "1,533,873" the second at 9.
        assert_eq!(lines[0], "  name        count  note");
        assert_eq!(lines[1], "  source          7  roll-up");
    }

    #[test]
    fn depth_indents_the_first_column_only() {
        let rendered = table().render();
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines[2], "    php   1,533,873  detail");
    }

    #[test]
    fn no_line_ends_in_whitespace() {
        let mut table = Table::new(vec![Column::left("name"), Column::left("note")]);
        table.push(0, cells(&["a-long-name", ""]));
        table.push(0, cells(&["b", "note"]));
        for line in table.render().lines() {
            assert_eq!(line, line.trim_end(), "trailing whitespace in {line:?}");
        }
    }

    #[test]
    fn a_short_row_is_padded_out_to_the_column_count() {
        let mut table = Table::new(vec![
            Column::left("name"),
            Column::left("provenance"),
            Column::right("count"),
        ]);
        table.push(0, cells(&["source"]));
        assert_eq!(table.render().lines().nth(1), Some("  source"));
    }

    #[test]
    fn a_table_without_rows_renders_nothing() {
        let table = Table::new(vec![Column::left("name")]);
        assert!(table.is_empty());
        assert_eq!(table.render(), "");
    }
}
