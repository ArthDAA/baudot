import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { HudApp } from "./HudApp";
import { SettingsApp } from "./SettingsApp";
import "./styles.css";

// Une seule app frontend : la vue est choisie selon le label de la fenêtre
// Tauri qui l'héberge ("hud" transparente, ou "settings" opaque).
const label = getCurrentWindow().label;
document.body.dataset.view = label;

// Thème résolu côté Rust ("system" -> lit la préférence claire/sombre de
// Windows). Diffusé aux deux fenêtres via l'évènement "theme" quand il change.
invoke<string>("get_effective_theme").then((theme) => {
  document.body.dataset.theme = theme;
});
listen<string>("theme", (e) => {
  document.body.dataset.theme = e.payload;
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {label === "settings" ? <SettingsApp /> : <HudApp />}
  </React.StrictMode>,
);
