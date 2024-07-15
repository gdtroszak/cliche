use std::{
    fs::{self},
    path::{Path, PathBuf},
};

use clap::Parser;
use fs_extra::dir::{self, CopyOptions};
use handlebars::Handlebars;
use pulldown_cmark::{html::push_html, CowStr, Event, Options, Tag};
use serde_json::Value;
use walkdir::WalkDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    generate_site(args)
}

fn extract_front_matter(
    content: &str,
) -> Result<(String, String, String), Box<dyn std::error::Error>> {
    // Check if the content starts with front matter delimiters
    if content.starts_with("---") {
        let parts: Vec<&str> = content.splitn(3, "---").collect();

        if parts.len() == 3 {
            // Parse the YAML front matter
            let front_matter_str = parts[1];
            let rest_content = parts[2];

            let front_matter: Value = serde_yaml::from_str(front_matter_str)
                .map_err(|e| format!("Failed to parse YAML front matter: {}", e))?;

            // Extract title from front matter, default to empty if not present
            let title = front_matter
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let meta_description = front_matter
                .get("meta_description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            Ok((
                title,
                meta_description,
                rest_content.trim_start().to_string(),
            ))
        } else {
            // Handle content without front matter or improperly formatted front matter
            Ok(("".to_string(), "".to_string(), content.to_string()))
        }
    } else {
        // Handle content without front matter
        Ok(("".to_string(), "".to_string(), content.to_string()))
    }
}

fn markdown_to_html(
    markdown_input: &str,
    content_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let parser = pulldown_cmark::Parser::new_ext(markdown_input, Options::all());
    let mut events: Vec<Event> = Vec::new();

    let content_dir_match = format!(
        "/{}",
        content_dir
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or("")
    );

    for event in parser {
        match event {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let dest_url = if dest_url.starts_with(&content_dir_match) {
                    dest_url.replace(&content_dir_match, "")
                } else {
                    dest_url.to_string()
                };
                let new_dest = if dest_url.ends_with(".md") {
                    if dest_url.ends_with("./index.md") {
                        "/".to_string()
                    } else if dest_url.ends_with("index.md") {
                        dest_url.replace("index.md", "")
                    } else {
                        dest_url.replace(".md", ".html")
                    }
                } else {
                    dest_url.to_string()
                };

                // Push the modified or original link event
                events.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: CowStr::Boxed(new_dest.into_boxed_str()),
                    title,
                    id,
                }));
            }
            _ => events.push(event),
        }
    }

    let mut html_output = String::new();
    push_html(&mut html_output, events.into_iter());
    Ok(html_output)
}

fn render_template(
    title: &str,
    description: &str,
    style: &str,
    header: Option<&str>,
    footer: Option<&str>,
    content: &str,
) -> Result<String, handlebars::RenderError> {
    let mut handlebars = Handlebars::new();
    handlebars.register_template_string("template", include_str!("./template.html"))?;

    let data = serde_json::json!({
        "title": title,
        "description": description,
        "style": style,
        "header": header.unwrap_or(""),
        "footer": footer.unwrap_or(""),
        "content": content
    });

    handlebars.render("template", &data)
}

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about=None)]
struct Args {
    /// Directory containing the site's content.
    content: PathBuf,

    /// Path to the site's header.
    #[arg(long, default_value = "./header.md")]
    header: PathBuf,

    /// Path to the site's footer.
    #[arg(long, default_value = "./footer.md")]
    footer: PathBuf,

    /// Path to the site's stylesheet.
    #[arg(long, default_value = "./style.css")]
    style: PathBuf,

    /// Site output directory. Will be created if it doesn't already exist.
    #[arg(short, long, default_value = "./_site")]
    output: PathBuf,
}

fn generate_site(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let content_dir = fs::canonicalize(&args.content).expect("Unable to find content directory.");
    // Try to canonicalize the path
    let site_dir = match fs::canonicalize(&args.output) {
        Ok(path) => {
            // Path exists and is canonicalized, remove contents
            fs::remove_dir_all(&path)?;
            fs::create_dir_all(&path)?;
            path
        }
        Err(_) => {
            // Path does not exist, create it and canonicalize it again
            fs::create_dir_all(&args.output)?;
            fs::canonicalize(&args.output)?
        }
    };

    // Copy static assets from content/static to site/static
    let static_dir = content_dir.join("static");
    let output_static_dir = site_dir.join("static");
    if static_dir.exists() {
        fs::create_dir_all(&output_static_dir)?;
        copy_directory(&static_dir, &output_static_dir)?;
    }

    // Load the style
    let style_path = Path::new(&args.style);
    let style = fs::read_to_string(&style_path)?;

    // Generate html for header
    let header_html = fs::metadata(&args.header).ok().and_then(|metadata| {
        if metadata.is_file() {
            fs::read_to_string(&args.header)
                .ok()
                .and_then(|content| markdown_to_html(&content, &content_dir).ok())
        } else {
            None
        }
    });

    // Generate html for footer
    let footer_html = fs::metadata(&args.footer).ok().and_then(|metadata| {
        if metadata.is_file() {
            fs::read_to_string(&args.footer)
                .ok()
                .and_then(|content| markdown_to_html(&content, &content_dir).ok())
        } else {
            None
        }
    });

    for entry in WalkDir::new(&content_dir)
        .into_iter()
        .filter_entry(|e| !e.path().starts_with(&static_dir) || !(&e.path() == &style_path))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str() != Some("nav.md"))
    {
        if entry.file_type().is_file() && entry.path().extension().map_or(false, |e| e == "md") {
            let md = fs::read_to_string(entry.path())?;
            let (title, description, md_content) = extract_front_matter(&md)?;
            let html = markdown_to_html(&md_content, &content_dir)?;

            let relative_path = entry
                .path()
                .strip_prefix(&content_dir)?
                .with_extension("html");
            let output_path = site_dir.join(&relative_path);

            let final_html = render_template(
                &title,
                &description,
                &style,
                header_html.as_deref(),
                footer_html.as_deref(),
                &html,
            )?;

            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(output_path, final_html)?;
        }
    }

    Ok(())
}

fn copy_directory(from: &Path, to: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut options = CopyOptions::new();
    options.overwrite = true;
    options.content_only = true;
    dir::copy(from, to, &options)?;
    Ok(())
}
