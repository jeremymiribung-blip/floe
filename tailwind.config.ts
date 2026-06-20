import type { Config } from "tailwindcss";

// Reference: actual runtime config lives in src/styles/globals.css via @theme (Tailwind v4).
// This file is kept for IDE support and structured token documentation.
const config: Config = {
  content: ["./src/**/*.{ts,tsx,js,jsx}", "./index.html"],
  darkMode: "class",
  theme: {
    extend: {
      fontFamily: {
        sans: [
          "Inter",
          "system-ui",
          "-apple-system",
          "BlinkMacSystemFont",
          '"SF Pro Text"',
          '"Segoe UI"',
          "sans-serif",
        ],
        mono: [
          '"JetBrains Mono"',
          '"SF Mono"',
          "Menlo",
          "Monaco",
          "Consolas",
          '"Liberation Mono"',
          "monospace",
        ],
      },
      colors: {
        floe: {
          // Backgrounds
          bg: {
            base: "#050614",
            elevated: "#050509",
            subtle: "#040513",
            overlay: "#050509B3",
            "recording-pill": "#050509CC",
          },
          // Surfaces
          surface: {
            muted: "#0B0C18",
            strong: "#111224",
            "border-subtle": "#1F1F2C",
            "border-strong": "#2C2C3A",
          },
          // Borders
          border: {
            subtle: "#1F1F2C",
            strong: "#323244",
            focus: "#52EEE5",
            danger: "#F97373",
          },
          // Text
          text: {
            primary: "#F5F5F7",
            secondary: "#C7CBD8",
            muted: "#7A7F91",
            "on-accent": "#010102",
            danger: "#FCA5A5",
            success: "#4ADE80",
          },
          // Accent (cyan)
          accent: {
            DEFAULT: "#52EEE5",
            soft: "#143C3A",
            muted: "#2A6461",
          },
          // Functional grays
          gray: {
            100: "#F9FAFB",
            300: "#D1D5DB",
            500: "#6B7280",
            700: "#374151",
            800: "#111827",
            900: "#020617",
          },
        },
      },
      fontSize: {
        xs: ["10px", { lineHeight: "14px", letterSpacing: "0.08em" }],
        sm: ["11px", { lineHeight: "16px", letterSpacing: "0.05em" }],
        base: ["12px", { lineHeight: "18px", letterSpacing: "0.03em" }],
        md: ["13px", { lineHeight: "18px", letterSpacing: "0.01em" }],
        lg: ["14px", { lineHeight: "20px", letterSpacing: "0" }],
        xl: ["18px", { lineHeight: "24px", letterSpacing: "-0.01em" }],
      },
      borderRadius: {
        xs: "3px",
        sm: "5px",
        md: "7px",
        lg: "9px",
        xl: "12px",
        full: "999px",
      },
      boxShadow: {
        "floe-soft": "0 16px 40px rgba(0, 0, 0, 0.55)",
        "floe-strong": "0 24px 80px rgba(0, 0, 0, 0.80)",
        "floe-focus-outer": "0 0 0 4px rgba(82, 238, 229, 0.20)",
        "floe-focus-inner": "0 0 0 1px #52EEE5",
        "floe-inner-card": "inset 0 1px 0 rgba(255,255,255,0.05)",
      },
    },
  },
  plugins: [],
};

export default config;
