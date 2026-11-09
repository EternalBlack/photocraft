//! Prompt enhancement: a cloud text model rewrites a prompt for the image models.
//!
//! - `ai.enhancePrompt` sends the prompt to the service chosen in Preferences › Integrations › AI ›
//!   Prompt Enhancement (`integrations.ai.enhance`: OpenRouter, OpenAI, xAI, Gemini or
//!   Anthropic, with a model of its own and a timeout) and returns the rewritten prompt. It is a
//!   background job: cancellable, and its progress says how long the answer has been awaited.
//!   It changes no document; the AI Gen panel puts the answer in its prompt box.
//! - `ai.enhanceModels` lists the models a service offers for it (the settings' picker).
//! - With `local` chosen, a text model on the user's running ComfyUI writes it. Models are
//!   installed and managed by the user alongside their other ComfyUI models.
//!
//! The keys are [`crate::ai_cmds::AiRuntime`]'s: OpenAI, xAI and Gemini share their image
//! models' keys; OpenRouter's and Anthropic's are kept beside them (`ai.setKey`).

use photocraft_aigen::enhance::{self, Ask, MAX_PROMPT, MAX_TIMEOUT, MAX_WORDS, MIN_TIMEOUT, MIN_WORDS, Purpose, local};
use photocraft_aigen::{Control, EnhanceSettings, TextProvider};
use serde_json::{Value, json};

use crate::ai_cmds::{ai_error, bad, settings, transport};
use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

/// The label of the job (the status bar's progress, Cancel).
pub const LABEL: &str = "Enhance Prompt";

fn provider_param(cmd: &str, p: &Value, chosen: TextProvider) -> Result<TextProvider> {
    match p.get("provider") {
        None => Ok(chosen),
        Some(v) => v.as_str().and_then(TextProvider::from_key).ok_or_else(|| bad(cmd, format!("`provider` must be {}", TextProvider::keys()))),
    }
}

fn enhance_prompt(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.enhancePrompt";
    let original = p.get("prompt").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| bad(cmd, "missing \"prompt\""))?.to_string();
    if original.chars().count() > MAX_PROMPT {
        return Err(bad(cmd, format!("the prompt is longer than {MAX_PROMPT} characters")));
    }
    let purpose = match p.get("mode") {
        None => Purpose::Create,
        Some(v) => v.as_str().and_then(Purpose::from_mode).ok_or_else(|| bad(cmd, "`mode` must be txt2img|imgEdit|inpaint|img2img"))?,
    };
    let ai = settings(s);
    let mut chosen = ai.enhance.clone();
    chosen.provider = provider_param(cmd, p, chosen.provider)?;
    if let Some(v) = p.get("textModel") {
        let model = v.as_str().map(str::trim).filter(|m| !m.is_empty() && m.len() <= 200).ok_or_else(|| bad(cmd, "`textModel` must be a model id"))?;
        if chosen.provider.is_local() && local::find(model).is_none() {
            return Err(bad(cmd, format!("`textModel` must be a local model: {}", local_ids())));
        }
        chosen.models.insert(chosen.provider.key().to_string(), model.to_string());
    }
    if let Some(v) = p.get("timeout") {
        let range = u64::from(MIN_TIMEOUT)..=u64::from(MAX_TIMEOUT);
        let seconds = v.as_u64().filter(|t| range.contains(t)).ok_or_else(|| bad(cmd, format!("`timeout` must be {MIN_TIMEOUT} to {MAX_TIMEOUT} seconds")))?;
        chosen.timeout_seconds = u32::try_from(seconds).unwrap_or(MAX_TIMEOUT);
    }
    if let Some(v) = p.get("words") {
        let range = u64::from(MIN_WORDS)..=u64::from(MAX_WORDS);
        let words = v.as_u64().filter(|w| range.contains(w)).ok_or_else(|| bad(cmd, format!("`words` must be {MIN_WORDS} to {MAX_WORDS}")))?;
        chosen.words = u32::try_from(words).unwrap_or(MAX_WORDS);
    }
    // The image model the prompt is for: its name tells the text model what it writes for.
    let image_model = match p.get("model") {
        None => String::new(),
        Some(v) => {
            let id = v.as_str().ok_or_else(|| bad(cmd, "`model` must be a model id"))?;
            ai.models
                .iter()
                .find(|m| m.id == id)
                .map(|m| if m.name.trim().is_empty() { m.family.label().to_string() } else { m.name.clone() })
                .unwrap_or_default()
        }
    };
    // The style preset the image is rendered in: the prompt is written for it.
    let style = match p.get("style") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) if id.trim().is_empty() => None,
        Some(Value::String(id)) => Some(
            photocraft_aigen::style::find(id)
                .ok_or_else(|| bad(cmd, format!("no style `{}`: ai.styles lists them", id.chars().take(64).collect::<String>())))?,
        ),
        Some(_) => return Err(bad(cmd, "`style` must be a style id (ai.styles lists them)")),
    };
    let transport = transport(s)?;
    let ask = Ask { prompt: original.clone(), purpose, image_model, style };
    if chosen.provider.is_local() {
        return enhance_locally(s, transport, chosen, ask, original);
    }
    let key = s.ai.text_key(chosen.provider);
    if key.is_empty() {
        return Err(EngineError::Other(format!("{} needs an API key: add one in Preferences › Integrations › AI › API Keys", chosen.provider.label())));
    }
    crate::jobs::run(
        s,
        LABEL,
        false,
        move |ctx| {
            let control = Control { cancelled: &|| ctx.cancelled(), progress: &|f, m| ctx.progress(f, m), sleep: &|d| std::thread::sleep(d) };
            enhance::enhance(&transport, &chosen, &key, &ask, &control).map_err(ai_error)
        },
        move |_, done| {
            Ok(json!({"prompt": done.prompt, "original": original, "provider": done.provider.key(), "textModel": done.model, "style": style.map(|s| s.id)}))
        },
    )
}

/// `local::MODELS`' ids, for errors.
fn local_ids() -> String {
    local::MODELS.map(|m| m.id).join("|")
}

/// Enhance with a text model on the user's running ComfyUI.
fn enhance_locally(
    s: &mut Session,
    transport: std::sync::Arc<dyn photocraft_aigen::Transport>,
    chosen: EnhanceSettings,
    ask: Ask,
    original: String,
) -> Result<Value> {
    let ai = settings(s);
    let style = ask.style;
    crate::jobs::run(
        s,
        LABEL,
        false,
        move |ctx| {
            let control = Control { cancelled: &|| ctx.cancelled(), progress: &|f, m| ctx.progress(f, m), sleep: &|d| std::thread::sleep(d) };
            local::enhance(transport.as_ref(), &ai.comfy_url, &chosen, &ask, photocraft_aigen::random_seed(), &control).map_err(ai_error)
        },
        move |_, done| {
            Ok(json!({"prompt": done.prompt, "original": original, "provider": done.provider.key(), "textModel": done.model, "style": style.map(|s| s.id)}))
        },
    )
}

fn enhance_local_models(s: &mut Session, _: &Value) -> Result<Value> {
    let models: Vec<Value> =
        local::MODELS.iter().map(|m| json!({"id": m.id, "name": m.name, "role": m.role, "file": m.file, "folder": local::FOLDER})).collect();
    Ok(json!({"models": models, "default": local::DEFAULT, "chosen": settings(s).enhance.model(TextProvider::Local)}))
}

fn enhance_models(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "ai.enhanceModels";
    let provider = provider_param(cmd, p, settings(s).enhance.provider)?;
    let transport = transport(s)?;
    let key = s.ai.text_key(provider);
    crate::jobs::run(
        s,
        "List Text Models",
        false,
        move |ctx| {
            ctx.progress(0.1, &format!("Asking {} for its models…", provider.short_label()));
            enhance::list_models(transport.as_ref(), provider, &key).map_err(ai_error)
        },
        move |_, models| Ok(json!({"provider": provider.key(), "models": models, "default": provider.default_model()})),
    )
}

fn can_enhance(s: &Session) -> std::result::Result<(), String> {
    if s.ai.transport.is_none() {
        return Err("prompt enhancement needs the desktop app".into());
    }
    Ok(())
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "ai.enhancePrompt",
            label: "Enhance Prompt",
            menu: &[],
            shortcut: None,
            params: r##"{"prompt":str,"mode":"txt2img|imgEdit|inpaint|img2img"="txt2img","model":id? (the image model it is for; its name is passed on),"provider":"openrouter|openai|xai|gemini|anthropic|local"?=integrations.ai.enhance.provider (local: text model on the configured ComfyUI; see ai.enhanceLocalModels),"textModel":str?=the preferences' model for that provider,"timeout":5..600?=integrations.ai.enhance.timeoutSeconds,"words":10..400?=integrations.ai.enhance.words,"style":id?=none (the prompt is written for that look)} → {prompt,original,provider,textModel,style}: a text model rewrites the prompt with more detail while retaining its meaning, names and quoted text. Background job; local models must already be installed in ComfyUI"##,
            enabled: can_enhance,
            run: enhance_prompt,
            journal: false,
        },
        CommandSpec {
            id: "ai.enhanceModels",
            label: "List Text Models",
            menu: &[],
            shortcut: None,
            params: r##"{"provider":"openrouter|openai|xai|gemini|anthropic"?=integrations.ai.enhance.provider} → {provider,models:[id,…] (the ones that answer in text, newest first where the provider dates them),default (the model used when the preferences name none)}: asks the provider with its saved key (OpenRouter's list needs none)"##,
            enabled: can_enhance,
            run: enhance_models,
            journal: false,
        },
        CommandSpec {
            id: "ai.enhanceLocalModels",
            label: "Local Text Models",
            menu: &[],
            shortcut: None,
            params: r##"{} → {models:[{id,name,role,file,folder}],default,chosen}: local text model choices. Install the selected model yourself in ComfyUI's text_encoders folder"##,
            enabled: |_| Ok(()),
            run: enhance_local_models,
            journal: false,
        },
    ]
}
