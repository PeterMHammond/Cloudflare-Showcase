use minijinja::{Environment, Error as MiniJinjaError};
use std::collections::HashMap;
use once_cell::sync::Lazy;

static TEMPLATES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut templates = HashMap::new();
    
    // Base template
    templates.insert("base.html", include_str!("../../templates/base.html"));
    
    // Component templates
    templates.insert("components/header.html", include_str!("../../templates/components/header.html"));
    templates.insert("components/sidebar.html", include_str!("../../templates/components/sidebar.html"));
    templates.insert("components/footer.html", include_str!("../../templates/components/footer.html"));
    templates.insert("components/logging.html", include_str!("../../templates/components/logging.html"));
    
    // Page templates
    templates.insert("index.html", include_str!("../../templates/index.html"));
    templates.insert("about.html", include_str!("../../templates/about.html"));
    templates.insert("openai.html", include_str!("../../templates/openai.html"));
    templates.insert("stt.html", include_str!("../../templates/stt.html"));
    templates.insert("study.html", include_str!("../../templates/study.html"));
    templates.insert("turnstile.html", include_str!("../../templates/turnstile.html"));
    templates.insert("verify.html", include_str!("../../templates/verify.html"));
    templates.insert("version.html", include_str!("../../templates/version.html"));
    templates.insert("websocket.html", include_str!("../../templates/websocket.html"));
    
    templates
});

pub fn create_environment() -> Result<Environment<'static>, MiniJinjaError> {
    let mut env = Environment::new();
    
    // Set up the loader
    env.set_loader(|name| {
        TEMPLATES.get(name).map(|content| Ok(content.to_string())).transpose()
    });
    
    // Add templates
    for (name, content) in TEMPLATES.iter() {
        env.add_template(name, content)?;
    }
    
    Ok(env)
}

pub fn render_template(name: &str, context: serde_json::Value) -> Result<String, String> {
    let env = create_environment().map_err(|e| format!("Failed to create environment: {}", e))?;
    
    let template = env.get_template(name).map_err(|e| format!("Failed to get template: {}", e))?;
    
    template.render(context).map_err(|e| format!("Failed to render template: {}", e))
}