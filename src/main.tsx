import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import FloatingWidget from "@/components/FloatingWidget";
import WidgetMenu from "@/components/WidgetMenu";
import DesktopWindow from "@/pages/DesktopWindow";
import "./index.css";

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
