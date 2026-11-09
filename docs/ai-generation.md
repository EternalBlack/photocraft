# AI generation (Window › AI Gen)

Generate an image from a prompt with a cloud provider or with a local model served by ComfyUI.
PhotoCraft sends requests to the configured provider and places the result on a new layer; it does
not bundle or install models.

## Getting started

1. Open **Edit › Preferences › AI Gen**. Add a cloud model and
   its provider API key, or configure the address of a ComfyUI server that you have installed and
   started yourself. Use **Connect** to read the server's model files and **Scan Server** to add
   detected models, including an entry for each checkpoint. The model list starts empty; use
   **Add Model** to configure one manually or **Restore Defaults** to add recommended models.
2. Open **Window › AI Gen**, choose a model, enter a prompt, and select **Generate**. With the
   prompt focused, ⌘/Ctrl+Return also starts generation. The gear opens the selected model's
   settings.
3. The generated layer covers the selection's bounds, or the full canvas if there is no selection.
   The selection is retained as a layer mask. Generation runs as a cancellable background job and
   can be undone as one operation.

The selection determines the output's shape. The model's **Render size (MP)** setting controls
how many pixels it generates; the result is scaled and cropped to the target area. A model marked
as supporting transparency can use the panel's **Alpha Channel** option.

## Generation and editing

The panel's **Create** mode generates from a prompt. **Edit** offers **Change**, which follows an
instruction to alter a layer or document, and **Restyle**, which redraws a source image while
following its composition. Depending on the model, Change can use an inpainting mask or edit the
source directly. The mask tools can use the Brush and Eraser to add or remove painted areas.

Models that support scene layout can use **Regions** to assign prompts to boxes and set their
front-to-back order. Editing models can use named image references from the document. **Advanced**
contains model-specific options, seeds, animation controls and per-model LoRAs. The batch menu
generates several images as separate layers. **Style** applies a preset to generation or prompt
enhancement, and **Enhance** asks a configured text model to expand the prompt while keeping its
intent. **Load from layer** restores the prompt and generation options stored on a generated layer.

## Models and credentials

The AI Gen preferences let you set the ComfyUI server URL and timeout, connect to the server,
detect model files, add or remove cloud and local models, and edit each model's settings. Scan
Server reports the number of checkpoints rather than the total number of files across every
ComfyUI model folder, and adds a model entry for every detected checkpoint. It guesses a supported
architecture from known checkpoint names when possible and otherwise configures a generic
checkpoint model. Local model settings include architecture-specific files such as HiDream O1 refiners and
Qwen inpainting ControlNets; cloud settings expose provider-specific quality and size options. The
supported local model architectures and their ComfyUI workflows are defined in
[`crates/aigen/src/model.rs`](../crates/aigen/src/model.rs) and
[`crates/aigen/src/comfy.rs`](../crates/aigen/src/comfy.rs). Cloud provider requests are in
[`crates/aigen/src/cloud.rs`](../crates/aigen/src/cloud.rs).

Cloud API keys are stored separately from preferences in `ai-keys.json` in PhotoCraft's settings
directory. They are never returned by `prefs.get`; `ai.keys` reports only whether a key is set.
When no saved key is present, the provider's environment variable can be used. Keys are sent only
to the corresponding provider. The key file is plain text; protect it like other credentials on
the computer.

Local models require a ComfyUI server and model files that the user has installed and launched.
PhotoCraft only connects to an already-running server; it does not install, start, stop, or manage
ComfyUI or its models, and has no first-start or recallable setup wizard. The local prompt-enhancement
models are managed in ComfyUI's `text_encoders` folder.

Generation metadata is kept with document layers. `.pcraft` documents persist it in the manifest;
PSD/PSB files use PhotoCraft metadata and image resources for generation details, references, and
regions.

## For automation

Generation is available through the engine command `ai.generate`; `ai.models` lists configured
models and readiness, `ai.comfyModels` reads a ComfyUI server's model files, and `ai.setKey` /
`ai.keys` manage and inspect API-key presence. AI settings live under `integrations.ai` and panel
state is exposed as `ai_gen` in the UI state.

## Code map

- [`crates/aigen/`](../crates/aigen/): model/settings types, ComfyUI workflows, cloud requests,
  model detection, image sizing, and the HTTP transport seam.
- [`crates/engine/src/ai_cmds.rs`](../crates/engine/src/ai_cmds.rs): generation and AI commands.
- [`crates/ui-egui/src/ai_gen_ui.rs`](../crates/ui-egui/src/ai_gen_ui.rs): the AI Gen panel and
  preferences controls.
- [`apps/photocraft/src/services.rs`](../apps/photocraft/src/services.rs): separate API-key file
  persistence; the native app supplies the HTTP transport.
