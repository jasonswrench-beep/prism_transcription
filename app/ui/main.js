// Phase 0: ask the shell whether the light pack is installed and say so.
const status = document.getElementById("models");
if (window.__TAURI__) {
  window.__TAURI__.core.invoke("model_status").then((s) => { status.textContent = s; }, (e) => { status.textContent = String(e); });
} else {
  status.textContent = "Open this page in the Prism Transcribe app to check the speech models.";
}
