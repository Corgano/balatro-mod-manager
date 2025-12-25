// ESLint flat config for Svelte + TypeScript
import js from "@eslint/js";
import svelte from "eslint-plugin-svelte";
import svelteParser from "svelte-eslint-parser";
import tsParser from "@typescript-eslint/parser";
import globals from "globals";

export default [
  {
    ignores: [
      "node_modules/",
      "build/",
      ".svelte-kit/",
      "dist/",
      ".flatpak-builder/",
      "src-tauri/target/",
      "target/",
      "vite.config.js.timestamp-*",
    ],
  },
  js.configs.recommended,
  // Svelte recommended rules
  ...svelte.configs["flat/recommended"],
  // Default: browser globals for app code
  {
    files: ["**/*.{js,ts,svelte}", "src/**/*"],
    languageOptions: {
      globals: {
        ...globals.browser,
      },
    },
  },
  {
    files: ["**/*.svelte"],
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        // Let the Svelte parser use TypeScript for <script lang="ts">
        parser: tsParser,
        extraFileExtensions: [".svelte"],
      },
    },
    rules: {
      // TS handles undefined symbols and unused vars in Svelte context
      "no-undef": "off",
      "no-unused-vars": "off",
    },
  },
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
    },
    rules: {
      // TS handles undefined symbols and unused vars
      "no-undef": "off",
      "no-unused-vars": [
        "warn",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  // Node globals for config files
  {
    files: ["vite.config.*", "eslint.config.*"],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
  },
  // Ignore unused vars in enums exported from stores
  {
    files: ["src/stores/modStore.ts"],
    rules: {
      "no-unused-vars": "off",
    },
  },
];
