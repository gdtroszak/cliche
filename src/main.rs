use std::{
    fs::{self},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::Parser;
use fs_extra::dir::{self, CopyOptions};
use handlebars::Handlebars;
use pulldown_cmark::{html::push_html, CowStr, Event, Options, Tag};
use serde_json::Value;
use shellexpand::tilde;
use walkdir::WalkDir;

fn main() {
    let args = Args::parse();
    generate_site(args)
}

/// Command line arguments for the application.
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about=None)]
struct Args {
    /// Directory containing the site's content.
    content: String,

    /// Path to the site's header.
    #[arg(long, default_value = "header.md")]
    header: String,

    /// Path to the site's footer.
    #[arg(long, default_value = "footer.md")]
    footer: String,

    /// Path to the site's stylesheet.
    #[arg(long, default_value = "style.css")]
    style: String,

    /// Site output directory. Will be created if it doesn't already exist.
    #[arg(short, long, default_value = "_site")]
    output: String,
}

/// Generates the static site using provided command line arguments.
///
/// # Arguments
/// * `args` - Parsed command line arguments.
fn generate_site(args: Args) {
    let content_path = get_content_path(args.content)
        .map_err(|e| eprintln!("Error extracting the content path: {}", e))
        .unwrap_or_else(|_| std::process::exit(1));
    let output_path = get_output_path(args.output)
        .map_err(|e| eprintln!("Error preparing the output path: {}", e))
        .unwrap_or_else(|_| std::process::exit(1));

    copy_static_assets(&content_path, &output_path)
        .map_err(|e| eprintln!("Error copying static assets: {}", e))
        .unwrap_or_else(|_| std::process::exit(1));

    let style = load_style(&args.style);
    let header = load_header(&args.header, &content_path);
    let footer = load_footer(&args.footer, &content_path);

    for entry in WalkDir::new(&content_path)
        .into_iter()
        .filter_entry(|e| !e.path().starts_with(&content_path.join("static")))
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() && entry.path().extension().map_or(false, |e| e == "md") {
            let html_content = load_html_from_md_file(entry.path(), &content_path)
                .map_err(|e| eprintln!("Error rendering markdown to HTML: {}", e))
                .unwrap_or_else(|_| std::process::exit(1));

            let relative_path = entry
                .path()
                .strip_prefix(&content_path)
                .unwrap()
                .with_extension("html");
            let output_path = output_path.join(&relative_path);

            let final_html = render_template(
                style.as_deref(),
                header.as_ref().map(|content| content.html.as_str()),
                footer.as_ref().map(|content| content.html.as_str()),
                html_content,
            )
            .map_err(|e| eprintln!("Error rendering template: {}", e))
            .unwrap_or_else(|_| std::process::exit(1));

            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| eprintln!("Error creating content directory: {}", e))
                    .unwrap_or_else(|_| std::process::exit(1));
            }
            fs::write(output_path, final_html)
                .map_err(|e| eprintln!("Error writing generated HTML to file: {}", e))
                .unwrap_or_else(|_| std::process::exit(1));
        }
    }
}

/// Retrieves the absolute path for the content directory, handling expansion of any user variables.
///
/// # Arguments
/// * `content_path` - The path to the content directory.
///
/// # Returns
/// * A `Result<PathBuf>` which is the absolute path of the content directory.
fn get_content_path(content_path: impl AsRef<str>) -> Result<PathBuf> {
    get_absolute_path(content_path)
}

/// Prepares the output directory for the site. If it exists, it is cleared; if not, it is created.
///
/// # Arguments
/// * `output_path` - The path to the output directory.
///
/// # Returns
/// * A `Result<PathBuf>` which is the prepared path of the output directory.
fn get_output_path(output_path: impl AsRef<str>) -> Result<PathBuf> {
    match get_absolute_path(&output_path) {
        Ok(path) => {
            // Path exists and is canonicalized, remove contents
            fs::remove_dir_all(&path)?;
            fs::create_dir_all(&path)?;
            Ok(path)
        }
        Err(_) => {
            // Path does not exist, create it and canonicalize it again
            let output_path = expand_path(output_path);
            fs::create_dir_all(&output_path)?;
            Ok(fs::canonicalize(&output_path)?)
        }
    }
}

/// Copies static assets from the content directory to the output directory.
///
/// # Arguments
/// * `content_path` - Path to the content directory containing static assets.
/// * `output_path` - Path to the output directory where static assets should be copied.
///
/// # Returns
/// * A `Result<()>` indicating the success or failure of the operation.
fn copy_static_assets(content_path: &Path, output_path: &Path) -> Result<()> {
    let static_dir = content_path.join("static");
    let output_static_dir = output_path.join("static");
    if static_dir.exists() {
        fs::create_dir_all(&output_static_dir)?;

        let mut options = CopyOptions::new();
        options.overwrite = true;
        options.content_only = true;
        dir::copy(&static_dir, &output_static_dir, &options)?;
    }
    Ok(())
}

/// Attempts to load the stylesheet from a specified path.
///
/// # Arguments
/// * `style_path` - Path to the stylesheet file.
///
/// # Returns
/// * An `Option<String>` containing the stylesheet content, or `None` if the file could not be read.
fn load_style(style_path: impl AsRef<str>) -> Option<String> {
    get_absolute_path(style_path)
        .ok()
        .and_then(|path| fs::read_to_string(&path).ok())
}

/// Loads and processes the header markdown file into HTML content.
///
/// # Arguments
/// * `header_path` - Path to the header markdown file.
/// * `content_path` - Path to the content directory for resolving relative paths.
///
/// # Returns
/// * An `Option<HTMLContent>` containing the processed header HTML content, or `None` if an error occurs.
fn load_header(header_path: impl AsRef<str>, content_path: &Path) -> Option<HTMLContent> {
    let header_path = get_absolute_path(header_path).ok()?;
    load_html_from_md_file(&header_path, content_path).ok()
}

/// Loads and processes the footer markdown file into HTML content.
///
/// # Arguments
/// * `footer_path` - Path to the footer markdown file.
/// * `content_path` - Path to the content directory for resolving relative paths.
///
/// # Returns
/// * An `Option<HTMLContent>` containing the processed footer HTML content, or `None` if an error occurs.
fn load_footer(footer_path: impl AsRef<str>, content_path: &Path) -> Option<HTMLContent> {
    let footer_path = get_absolute_path(footer_path).ok()?;
    load_html_from_md_file(&footer_path, content_path).ok()
}

/// Converts a given markdown file's contents to HTML, incorporating the site's layout.
///
/// # Arguments
/// * `path` - Path to the markdown file.
/// * `content_path` - Path to the content directory for resolving relative paths.
///
/// # Returns
/// * A `Result<HTMLContent>` containing the HTML content or an error if conversion fails.
fn load_html_from_md_file(path: &Path, content_path: &Path) -> Result<HTMLContent> {
    fs::read_to_string(&path)
        .with_context(|| format!("Failed to read from markdown file: {:?}", path))
        .and_then(|file_content| process_markdown(&file_content))
        .with_context(|| "Failed to process markdown file.")
        .and_then(|markdown_content| {
            let html = markdown_to_html(&markdown_content.markdown, content_path)
                .with_context(|| "Failed to convert markdown to HTML.")?;
            Ok(HTMLContent {
                front_matter: markdown_content.front_matter,
                html,
            })
        })
}

/// Renders the final HTML content using a template and dynamic parts such as style, header, and footer.
///
/// # Arguments
/// * `style` - Optional stylesheet content.
/// * `header` - Optional header content.
/// * `footer` - Optional footer content.
/// * `content` - Main content of the HTML.
///
/// # Returns
/// * A `Result<String>` containing the final rendered HTML or an error if rendering fails.
fn render_template(
    style: Option<&str>,
    header: Option<&str>,
    footer: Option<&str>,
    content: HTMLContent,
) -> Result<String> {
    let mut handlebars = Handlebars::new();
    handlebars.register_template_string("template", include_str!("./template.html"))?;

    let data = serde_json::json!({
        "title": content.front_matter.as_ref().map_or("", |fm| fm.title.as_deref().unwrap_or("")),
        "description": content.front_matter.as_ref().map_or("", |fm| fm.description.as_deref().unwrap_or("")) ,
        "style": style.as_deref().unwrap_or(""),
        "header": header.as_deref().unwrap_or(""),
        "footer": footer.as_deref().unwrap_or(""),
        "content": content.html
    });

    Ok(handlebars.render("template", &data)?)
}

/// Converts a relative or tilde-expanded path to an absolute path.
///
/// # Arguments
/// * `path` - The path to be expanded and resolved.
///
/// # Returns
/// * A `Result<PathBuf>` containing the absolute path or an error if the path cannot be resolved.
fn get_absolute_path(path: impl AsRef<str>) -> Result<PathBuf> {
    Ok(fs::canonicalize(expand_path(path))?)
}

/// Expands the provided path, handling tilde notation for the home directory.
///
/// # Arguments
/// * `path` - The path to expand.
///
/// # Returns
/// * An expanded `PathBuf`.
fn expand_path(path: impl AsRef<str>) -> String {
    tilde(path.as_ref()).into_owned()
}

/// Data structure for holding front matter extracted from markdown.
struct FrontMatter {
    title: Option<String>,
    description: Option<String>,
}

/// Data structure for holding markdown content, potentially including extracted front matter.
struct MarkdownContent {
    front_matter: Option<FrontMatter>,
    markdown: String,
}

/// Data structure for holding HTML content, potentially including metadata from front matter.
struct HTMLContent {
    front_matter: Option<FrontMatter>,
    html: String,
}

/// Processes markdown content, extracting front matter if present, and converts it to structured content.
///
/// # Arguments
/// * content - The markdown content to process.
/// * content_dir - Path to the content directory for resolving relative paths.
///
/// # Returns
/// * A Result<MarkdownContent> containing structured markdown and extracted front matter.
fn process_markdown(content: &str) -> Result<MarkdownContent> {
    // Check if the content starts with front matter delimiters
    if content.starts_with("---") {
        let parts: Vec<&str> = content.splitn(3, "---").collect();

        if parts.len() == 3 {
            // Parse the YAML front matter
            let front_matter_str = parts[1];
            let rest_content = parts[2];

            let front_matter: Value = serde_yaml::from_str(front_matter_str)
                .with_context(|| "Failed to parse YAML front matter.")?;

            // Extract title from front matter, default to empty if not present
            let title = front_matter.get("title").map(Value::to_string);
            let meta_description = front_matter.get("meta_description").map(Value::to_string);

            Ok(MarkdownContent {
                front_matter: Some(FrontMatter {
                    title,
                    description: meta_description,
                }),
                markdown: rest_content.trim_start().to_string(),
            })
        } else {
            Ok(MarkdownContent {
                front_matter: None,
                markdown: content.to_string(),
            })
        }
    } else {
        Ok(MarkdownContent {
            front_matter: None,
            markdown: content.to_string(),
        })
    }
}

/// Converts markdown text to HTML format using a specified content directory to resolve paths.
///
/// # Arguments
/// * markdown_input - The markdown text to convert.
/// * content_dir - The content directory used for path resolution in the markdown.
///
/// # Returns
/// * A Result<String> containing the converted HTML text or an error if the conversion fails.
fn markdown_to_html(markdown_input: &str, content_dir: &Path) -> Result<String> {
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
