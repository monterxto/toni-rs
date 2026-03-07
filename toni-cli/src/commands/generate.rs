use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use regex::Regex;
use std::path::PathBuf;
use tokio::fs::{self};

#[derive(clap::Args)]
pub struct GenerateArgs {
    resource_type: String,
    name: String,
}

pub async fn execute(args: GenerateArgs) -> anyhow::Result<()> {
    match args.resource_type.to_lowercase().as_str() {
        "resource" => generate_resource(&args.name).await,
        _ => Err(anyhow!("Invalid resource type")),
    }
}

async fn generate_resource(name: &str) -> anyhow::Result<()> {
    let base_path = PathBuf::from("src").join("app").join(name);

    create_resource_structure(&base_path).await?;

    generate_resource_files(&base_path, name).await?;

    update_app_module(name).await?;

    update_mod_file(name).await?;

    println!(
        "{}",
        format!("✅ Successfully generated resource '{}'!", name).green()
    );

    Ok(())
}

async fn create_resource_structure(base_path: &PathBuf) -> Result<()> {
    fs::create_dir_all(base_path)
        .await
        .context("Failed to create resource directory")?;
    Ok(())
}

async fn generate_resource_files(base_path: &PathBuf, name: &str) -> Result<()> {
    let snake_case = to_snake_case(name);
    let upper_case_first_letter = to_upper_case_first_letter(name);

    let service_name = format!("{}Service", &upper_case_first_letter);
    let controller_name = format!("{}ControllerFactory", &upper_case_first_letter);
    let module_name = format!("{}Module", &upper_case_first_letter);
    let service_name_snake_case = format!("{}_service", &snake_case);
    let controller_name_snake_case = format!("{}_controller", &snake_case);
    let module_name_snake_case = format!("{}_module", &snake_case);

    let path_controller = format!("{}.controller.rs", name);
    let path_service = format!("{}.service.rs", name);
    let path_module = format!("{}.module.rs", name);

    let replacements: &[(&str, &str); 10] = &[
        ("resource_name", &snake_case),
        ("RESOURCE_NAME_SERVICE", &service_name),
        ("RESOURCE_NAME_CONTROLLER", &controller_name),
        ("RESOURCE_NAME_MODULE", &module_name),
        ("resource_name_service", &service_name_snake_case),
        ("resource_name_controller", &controller_name_snake_case),
        ("resource_name_module", &module_name_snake_case),
        ("path_module", &path_module),
        ("path_controller", &path_controller),
        ("path_service", &path_service),
    ];

    write_template_file(
        base_path,
        &format!("{}.module.rs", name),
        include_str!("../templates/generate/resource.module.rs"),
        replacements,
    )
    .await?;

    write_template_file(
        base_path,
        &format!("{}.controller.rs", name),
        include_str!("../templates/generate/resource.controller.rs"),
        replacements,
    )
    .await?;

    write_template_file(
        base_path,
        &format!("{}.service.rs", name),
        include_str!("../templates/generate/resource.service.rs"),
        replacements,
    )
    .await?;

    write_template_file(
        base_path,
        "mod.rs",
        include_str!("../templates/generate/mod.rs"),
        replacements,
    )
    .await?;

    Ok(())
}

async fn write_template_file(
    base_path: &PathBuf,
    filename: &str,
    template: &str,
    replacements: &[(&str, &str)],
) -> Result<()> {
    let path = base_path.join(filename);
    let mut content = template.to_string();

    for (placeholder, value) in replacements {
        content = content.replace(placeholder, value);
    }

    if content.contains("{{") || content.contains("}}") {
        return Err(anyhow!(
            "Template placeholders not fully replaced in {}",
            filename
        ));
    }

    fs::write(&path, content)
        .await
        .context(format!("Failed to write {}", path.display()))?;

    Ok(())
}

async fn update_app_module(resource_name: &str) -> Result<()> {
    let snake_case = to_snake_case(resource_name);
    let upper_case_first_letter = to_upper_case_first_letter(resource_name);
    let app_module_path = PathBuf::from("src").join("app").join("app.module.rs");

    let mut content = fs::read_to_string(&app_module_path)
        .await
        .context("Failed to read app.module.rs")?;

    let module_import = format!("use super::{}::{}_module::*;", resource_name, snake_case);
    if !content.contains(&module_import) {
        content = content.replacen(
            "use toni_macros::module;",
            &format!("use toni_macros::module;\n{}", module_import),
            1,
        );
    }

    let module_insert = format!("{}Module", upper_case_first_letter);
    if !content.contains(&module_insert) {
        content = add_module_to_imports(&content, &module_insert);
    }

    fs::write(&app_module_path, content)
        .await
        .context("Failed to update app.module.rs")?;

    Ok(())
}

async fn update_mod_file(resource_name: &str) -> Result<()> {
    let snake_case = to_snake_case(resource_name);
    let mod_path = PathBuf::from("src").join("app").join("mod.rs");
    let mut content = fs::read_to_string(&mod_path)
        .await
        .context("Failed to read mod.rs")?;

    let mod_insert = format!(r#"#[path = "{}/mod.rs"]"#, snake_case);
    if !content.contains(&mod_insert) {
        let part1 = format!(r#"#[path = "{}/mod.rs"]"#, snake_case);
        let part2 = format!("pub mod {};", snake_case);
        let part3 = r#"#[path = "app.module.rs"]"#;
        let output = format!("{}\n{}\n\n{}", part1, part2, part3);
        content = content.replacen(r#"#[path = "app.module.rs"]"#, &output, 1);
    }

    fs::write(&mod_path, content)
        .await
        .context("Failed to update mod.rs")?;

    Ok(())
}

fn add_module_to_imports(content: &str, module_name: &str) -> String {
    let regex_module = Regex::new(r"(?s)imports:\s*\[(.*?)\]").unwrap();
    let imports_content = regex_module
        .captures(content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str())
        .unwrap();

    let mut modules: Vec<&str> = imports_content
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    modules.push(module_name);

    let new_imports = format!("imports: [\n    {},\n  ]", modules.join(",\n    "));
    let output = regex_module.replace(content, new_imports);
    output.to_string()
}

fn _to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    for part in s.split('_') {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            result.push(first.to_ascii_uppercase());
            result.extend(chars.as_str().to_ascii_lowercase().chars());
        }
    }
    result
}

fn to_upper_case_first_letter(s: &str) -> String {
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        return format!("{}{}", first.to_ascii_uppercase(), chars.as_str());
    }
    s.to_string()
}

fn to_snake_case(input: &str) -> String {
    let mut snake_case = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c.is_uppercase() {
            if !snake_case.is_empty() {
                snake_case.push('_');
            }
            snake_case.push(c.to_ascii_lowercase());
        } else if c == ' ' {
            if !snake_case.is_empty() && snake_case.chars().last() != Some('_') {
                snake_case.push('_');
            }
        } else {
            snake_case.push(c);
        }

        if let Some(&next_char) = chars.peek() {
            if next_char.is_uppercase() && c != ' ' && c != '_' {
                snake_case.push('_');
            }
        }
    }

    snake_case
}
