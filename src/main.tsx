import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import FloatingWidget from "@/components/FloatingWidget";
import WidgetMenu from "@/components/WidgetMenu";
import DesktopWindow from "@/pages/DesktopWindow";
import "./index.css";

// WebView2 offers its own Back / Refresh / Save as / Inspect menu on right-click.
// The widget's right-click menu is a Piplo window (M5), so the native one is
// never wanted and would render over it. Devtools stay reachable via F12.
window.addEventListener("contextmenu", (event) => event.preventDefault());

// One bundle, three entry points — every window loads index.html and picks its
// root from its own label.
const ROOTS: Record<string, React.ComponentType> = {
  widget: FloatingWidget,
  menu: WidgetMenu,
  home: DesktopWindow,
};

const label = getCurrentWindow().label;
const Root = ROOTS[label];

if (!Root) {
  throw new Error(`no root component for window '${label}'`);
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
