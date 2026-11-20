//! Prints the settings row of a project database (a copy, read-only).
fn main() -> Result<(), String> {
    let db = std::env::args().nth(1).ok_or("project.db")?;
    let reader = qnc_db::Reader::open(std::path::Path::new(&db))?;
    let settings = qnc_project_settings::read(&reader)?;
    println!("{}", serde_json::to_string_pretty(&settings.saved).map_err(|e| e.to_string())?);
    Ok(())
}
