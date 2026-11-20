// Precision, dark green: the design's tokens as an Astryx theme, light and dark. `pnpm theme:build`
// turns this into precision.css/.js; never edit those by hand.

import { defineTheme } from "@astryxdesign/core/theme";
import { neutralTheme } from "@astryxdesign/theme-neutral";

const SHADOW_POP = "0 16px 48px light-dark(rgb(0 0 0 / 0.16), rgb(0 0 0 / 0.5))";
const EDGE = "inset 0 0 0 1px light-dark(rgb(0 0 0 / 0.08), rgb(255 255 255 / 0.07))";

export const precisionTheme = defineTheme({
  name: "precision",
  extends: neutralTheme,
  typography: {
    scale: { base: 13, ratio: 1.15 },
    body: { family: "Inter Variable", fallbacks: "system-ui, sans-serif" },
    heading: { family: "Inter Variable", fallbacks: "system-ui, sans-serif" },
    code: { family: "JetBrains Mono Variable", fallbacks: "ui-monospace, monospace" },
  },
  // The neutral theme raises the base to 16px on narrow touch screens: Precision stays at 13.
  adaptations: {
    rules: [
      {
        when: { pointer: "coarse", width: { below: "lg" } },
        value: { typography: { scale: { base: 13, ratio: 1.15 } } },
      },
    ],
  },
  motion: { fast: 100, medium: 160, slow: 300, ratio: 0.75 },
  localTokens: {
    "--precision-sunk": ["#f8f8f7", "#0b0c0d"],
    "--precision-line2": ["rgb(0 0 0 / 0.06)", "rgb(255 255 255 / 0.05)"],
    "--precision-text3": ["#696d74", "#7d828a"],
    "--precision-acc-line": ["#145536", "#2a8a5a"],
    "--precision-kbd": ["rgb(0 0 0 / 0.06)", "rgb(255 255 255 / 0.08)"],
    "--precision-dot": ["rgba(0, 0, 0, 0.07)", "rgba(255, 255, 255, 0.05)"],
    "--precision-connector": ["rgba(0, 0, 0, 0.16)", "rgba(255, 255, 255, 0.14)"],
    "--precision-bar": ["#8dbfa4", "#2e6b4c"],
    "--precision-panel-shadow": ["0 1px 2px rgb(0 0 0 / 0.04)", "none"],
    "--precision-card-shadow": ["0 1px 2px rgb(0 0 0 / 0.05)", "none"],
    "--precision-amber-soft": ["rgb(196 122 0 / 0.08)", "rgb(229 162 58 / 0.1)"],
    "--precision-violet": ["#6d4ac9", "#b39dfa"],
    // A small mark that must read as orange on both themes (a session's finished, unopened answer).
    "--precision-dot-orange": ["#e8730c", "#ff9e55"],
  },
  tokens: {
    "--color-background-body": ["#f3f3f2", "#08090a"],
    "--color-background-surface": ["#ffffff", "#0f1012"],
    "--color-background-card": ["#ffffff", "#16171a"],
    "--color-background-popover": ["#ffffff", "#141517"],
    "--color-background-muted": ["#f8f8f7", "#0f1012"],

    "--color-border": ["rgb(0 0 0 / 0.08)", "rgb(255 255 255 / 0.07)"],
    "--color-border-emphasized": ["rgb(0 0 0 / 0.12)", "rgb(255 255 255 / 0.09)"],

    "--color-overlay-hover": ["rgb(0 0 0 / 0.05)", "rgb(255 255 255 / 0.06)"],
    "--color-overlay-pressed": ["rgb(0 0 0 / 0.08)", "rgb(255 255 255 / 0.1)"],
    "--color-neutral": ["rgb(0 0 0 / 0.05)", "rgb(255 255 255 / 0.06)"],

    "--color-track": ["rgb(0 0 0 / 0.08)", "rgb(255 255 255 / 0.08)"],
    "--color-skeleton": ["rgb(0 0 0 / 0.08)", "rgb(255 255 255 / 0.08)"],

    "--color-text-primary": ["#1b1c1e", "#edeef0"],
    "--color-text-secondary": ["#55595f", "#a0a4ab"],
    "--color-text-disabled": ["#a6a9ae", "#4a4e55"],
    "--color-icon-primary": ["#1b1c1e", "#edeef0"],
    "--color-icon-secondary": ["#55595f", "#a0a4ab"],
    "--color-icon-disabled": ["#a6a9ae", "#4a4e55"],

    "--color-accent": ["#17603f", "#1f7a4c"],
    "--color-accent-muted": ["rgb(23 96 63 / 0.08)", "rgb(95 203 149 / 0.12)"],
    "--color-text-accent": ["#17603f", "#5fcb95"],
    "--color-icon-accent": ["#17603f", "#5fcb95"],
    "--color-on-accent": ["#FFFFFF", "#FFFFFF"],

    "--color-warning": ["#9a5800", "#e8a84a"],
    "--color-warning-muted": ["rgb(196 122 0 / 0.08)", "rgb(229 162 58 / 0.1)"],
    "--color-text-warning": ["#9a5800", "#e8a84a"],
    "--color-on-warning-muted": ["#9a5800", "#e8a84a"],
    "--color-border-warning": ["rgb(196 122 0 / 0.35)", "rgb(229 162 58 / 0.35)"],
    "--color-text-yellow": ["#9a5800", "#e8a84a"],
    "--color-icon-yellow": ["#9a5800", "#e8a84a"],

    "--color-error": ["#c2281c", "#f08a8a"],
    "--color-error-muted": ["rgb(194 40 28 / 0.08)", "rgb(240 138 138 / 0.12)"],
    "--color-text-error": ["#c2281c", "#f08a8a"],
    "--color-on-error-muted": ["#c2281c", "#f08a8a"],
    "--color-text-red": ["#c2281c", "#f08a8a"],
    "--color-icon-red": ["#c2281c", "#f08a8a"],

    "--color-info": ["#2d5bd0", "#7ea6f8"],
    "--color-info-muted": ["rgb(45 91 208 / 0.08)", "rgb(126 166 248 / 0.12)"],
    "--color-text-info": ["#2d5bd0", "#7ea6f8"],
    "--color-on-info-muted": ["#2d5bd0", "#7ea6f8"],
    "--color-text-blue": ["#2d5bd0", "#7ea6f8"],
    "--color-icon-blue": ["#2d5bd0", "#7ea6f8"],
    "--color-text-purple": ["#6d4ac9", "#b39dfa"],
    "--color-icon-purple": ["#6d4ac9", "#b39dfa"],

    "--color-success": ["#17603f", "#5fcb95"],
    "--color-success-muted": ["rgb(23 96 63 / 0.08)", "rgb(95 203 149 / 0.12)"],
    "--color-text-success": ["#17603f", "#5fcb95"],
    "--color-on-success-muted": ["#17603f", "#5fcb95"],
    "--color-text-green": ["#17603f", "#5fcb95"],
    "--color-icon-green": ["#17603f", "#5fcb95"],

    "--focus-outline-color": ["#17603f", "#5fcb95"],
    "--focus-outline-offset": "1px",

    "--radius-inner": "4px",
    "--radius-element": "6px",
    "--radius-container": "8px",
    "--radius-page": "12px",
    "--radius-chat": "12px",

    "--shadow-low": "0 1px 2px light-dark(rgb(0 0 0 / 0.05), rgb(0 0 0 / 0.3))",
    "--shadow-med": `${SHADOW_POP}, ${EDGE}`,
    "--shadow-high": `${SHADOW_POP}, ${EDGE}`,
  },
  components: {
    button: {
      base: { borderWidth: "1px", borderStyle: "solid", borderColor: "var(--color-border-emphasized)" },
      "variant:ghost": { borderColor: "transparent", backgroundColor: "transparent" },
      "variant:primary": { borderColor: "var(--precision-acc-line)" },
      "variant:destructive": {
        borderColor: "var(--color-error)",
        backgroundColor: "transparent",
        color: "var(--color-text-error)",
      },
    },
    item: {
      selected: { boxShadow: "inset 2px 0 0 var(--color-text-accent)" },
    },
    kbd: {
      base: { backgroundColor: "var(--precision-kbd)" },
    },
    "dropdown-menu": {
      base: { boxShadow: `${SHADOW_POP}, ${EDGE}` },
    },
    "command-palette-input": {
      base: { fontSize: "14px" },
    },
  },
});
