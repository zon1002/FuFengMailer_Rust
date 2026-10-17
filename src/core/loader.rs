//! 讀取員工清單 Excel（第一個工作表、第 1 列為標題，需含 Emp / NameKey / Email 欄），
//! 並依 {工號}-{年}.{月}*.pdf 規則配對薪資單。

use std::path::Path;

use calamine::{Data, Reader, open_workbook_auto};

use super::employee::Employee;

const MAX_HEADER_COLUMNS: u32 = 30;

#[derive(Debug)]
pub struct LoadResult {
    pub employees: Vec<Employee>,
    pub messages: Vec<String>,
    pub success: bool,
}

pub fn load(xlsx_path: &Path, pdf_folder: &Path, year: &str, month: &str) -> LoadResult {
    let mut messages = Vec::new();
    match load_inner(xlsx_path, pdf_folder, year, month, &mut messages) {
        Ok(employees) => {
            let with_pdf = employees.iter().filter(|e| !e.pdf_files.is_empty()).count();
            messages.push(format!(
                "[OK] 載入完成：共 {} 筆，其中 {with_pdf} 筆有對應 PDF（{year}年{month}月）",
                employees.len()
            ));
            LoadResult { employees, messages, success: true }
        }
        Err(msg) => {
            messages.push(msg);
            LoadResult { employees: Vec::new(), messages, success: false }
        }
    }
}

fn load_inner(xlsx: &Path, pdf_folder: &Path, year: &str, month: &str, messages: &mut Vec<String>) -> Result<Vec<Employee>, String> {
    if xlsx.as_os_str().is_empty() {
        return Err("[錯誤] 請先選擇 Excel 清單路徑".into());
    }
    if !xlsx.is_file() {
        return Err(format!("[錯誤] Excel 清單不存在: {}", xlsx.display()));
    }
    if pdf_folder.as_os_str().is_empty() {
        return Err("[錯誤] 請先選擇 PDF 資料夾路徑".into());
    }
    if !pdf_folder.is_dir() {
        return Err(format!("[錯誤] PDF 資料夾不存在: {}", pdf_folder.display()));
    }

    // Rust 在 Windows 以共用讀寫模式開檔，Excel 正開著這個檔案時仍可讀取
    let mut workbook = open_workbook_auto(xlsx).map_err(|e| format!("[錯誤] 讀取 Excel 錯誤: {e}"))?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or("[錯誤] Excel 沒有工作表")?
        .map_err(|e| format!("[錯誤] 讀取 Excel 錯誤: {e}"))?;
    // get_value 使用工作表的絕對座標（0 起算），不受資料起始位置影響
    let cell = |r: u32, c: u32| range.get_value((r, c)).map(cell_text).unwrap_or_default();

    let (mut col_emp, mut col_name, mut col_email) = (None, None, None);
    for c in 0..MAX_HEADER_COLUMNS {
        let header = cell(0, c);
        if header.is_empty() {
            break;
        }
        match header.as_str() {
            "Emp" => col_emp = Some(c),
            "NameKey" => col_name = Some(c),
            "Email" => col_email = Some(c),
            _ => {}
        }
    }
    let col_emp = col_emp.ok_or("[錯誤] 找不到欄位 'Emp'")?;
    let col_name = col_name.ok_or("[錯誤] 找不到欄位 'NameKey'")?;
    let col_email = col_email.ok_or("[錯誤] 找不到欄位 'Email'")?;

    let last_row = range.end().map_or(0, |(r, _)| r);
    let mut employees = Vec::new();
    for r in 1..=last_row {
        let emp_id = cell(r, col_emp);
        if emp_id.is_empty() {
            break; // 與 C++ 版相同：遇到空白工號即視為清單結束
        }
        let mut name = cell(r, col_name);
        if name.is_empty() {
            name = "(未知)".into();
        }
        let email = cell(r, col_email);
        if !email.contains('@') {
            messages.push(format!("[!] {emp_id} Email 格式不正確: {email}，跳過"));
            continue;
        }

        let mut emp = Employee::new(emp_id, name, email);
        match find_payslips(pdf_folder, &emp.emp_id, year, month) {
            Ok(files) => emp.pdf_files = files,
            Err(e) => messages.push(format!("[!] 掃描 PDF 資料夾時發生錯誤: {e}")),
        }
        emp.checked = !emp.pdf_files.is_empty();
        emp.refresh_pending_status(false);
        employees.push(emp);
    }
    Ok(employees)
}

/// 找出 `pdf_folder` 中檔名以「{工號}-{年}.{月}」開頭的 PDF（不含子資料夾），依檔名排序。
pub fn find_payslips(pdf_folder: &Path, emp_id: &str, year: &str, month: &str) -> std::io::Result<Vec<String>> {
    let prefix = format!("{emp_id}-{year}.{month}");
    let mut files: Vec<String> = std::fs::read_dir(pdf_folder)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
                && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
        })
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    files.sort_by_key(|f| f.to_lowercase());
    Ok(files)
}

/// 儲存格轉字串；整數數值不帶小數（工號 1001 不會變成 "1001.0"）。
pub(crate) fn cell_text(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        Data::Float(f) => f.to_string(),
        Data::String(s) => s.trim().to_string(),
        other => other.to_string().trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_xlsxwriter::Workbook;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), "%PDF").unwrap();
    }

    fn header(ws: &mut rust_xlsxwriter::Worksheet) {
        ws.write(0, 0, "Emp").unwrap();
        ws.write(0, 1, "Dept").unwrap();
        ws.write(0, 2, "NameKey").unwrap();
        ws.write(0, 3, "Email").unwrap();
    }

    #[test]
    fn loads_employees_and_matches_payslips() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("PaySlips");
        std::fs::create_dir(&pdf).unwrap();
        touch(&pdf, "1001-2026.09_薪資單.pdf");
        touch(&pdf, "1001-2026.09_補發.PDF");
        touch(&pdf, "1001-2026.08.pdf"); // 月份不同
        touch(&pdf, "1001-2026.09.txt"); // 副檔名不同
        touch(&pdf, "A002-2026.09.pdf");

        let xlsx = dir.path().join("passwords.xlsx");
        let mut wb = Workbook::new();
        let ws = wb.add_worksheet();
        header(ws);
        ws.write(1, 0, 1001).unwrap(); // 數值工號
        ws.write(1, 2, "王小明").unwrap();
        ws.write(1, 3, "ming@example.com").unwrap();
        ws.write(2, 0, "A002").unwrap();
        ws.write(2, 3, "a002@example.com").unwrap(); // 姓名空白
        ws.write(3, 0, "A003").unwrap();
        ws.write(3, 2, "沒信箱").unwrap();
        ws.write(3, 3, "not-an-email").unwrap();
        ws.write(4, 0, "A004").unwrap();
        ws.write(4, 2, "陳大文").unwrap();
        ws.write(4, 3, "chen@example.com").unwrap();
        ws.write(6, 0, "A999").unwrap(); // 第 6 列空白後的資料不讀
        ws.write(6, 3, "x@example.com").unwrap();
        wb.save(&xlsx).unwrap();

        let r = load(&xlsx, &pdf, "2026", "09");
        assert!(r.success, "{:?}", r.messages);
        let ids: Vec<_> = r.employees.iter().map(|e| e.emp_id.as_str()).collect();
        assert_eq!(ids, ["1001", "A002", "A004"]);

        let ming = &r.employees[0];
        assert_eq!(ming.pdf_files.len(), 2);
        assert!(ming.checked);
        assert_eq!(ming.status, "2個PDF，待寄");
        assert_eq!(r.employees[1].name_key, "(未知)");
        assert!(!r.employees[2].checked);
        assert_eq!(r.employees[2].status, "[X] 不寄送");
        assert!(r.messages.iter().any(|m| m.contains("A003") && m.contains("Email 格式不正確")));
        assert!(r.messages.last().unwrap().contains("共 3 筆，其中 2 筆有對應 PDF（2026年09月）"));
    }

    #[test]
    fn missing_column_fails() {
        let dir = tempfile::tempdir().unwrap();
        let xlsx = dir.path().join("x.xlsx");
        let mut wb = Workbook::new();
        let ws = wb.add_worksheet();
        ws.write(0, 0, "Emp").unwrap();
        ws.write(0, 1, "NameKey").unwrap();
        wb.save(&xlsx).unwrap();

        let r = load(&xlsx, dir.path(), "2026", "09");
        assert!(!r.success);
        assert_eq!(r.messages, ["[錯誤] 找不到欄位 'Email'"]);
    }

    #[test]
    fn missing_paths_fail_before_opening_excel() {
        assert_eq!(load(Path::new(""), Path::new("x"), "2026", "09").messages, ["[錯誤] 請先選擇 Excel 清單路徑"]);
        assert!(load(Path::new("Z:\\nope.xlsx"), Path::new("x"), "2026", "09").messages[0].starts_with("[錯誤] Excel 清單不存在"));
    }
}
