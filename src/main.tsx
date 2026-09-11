import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { NoticeProvider } from "./NoticeCenter";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <NoticeProvider>
      <App />
    </NoticeProvider>
  </StrictMode>,
);
