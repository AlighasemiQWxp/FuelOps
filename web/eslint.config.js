import js from "@eslint/js";
import tseslint from "typescript-eslint";
import hooks from "eslint-plugin-react-hooks";
export default tseslint.config(
  { ignores: ["web/dist/**", "**/node_modules/**"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    plugins: { "react-hooks": hooks },
    languageOptions: {
      globals: {
        navigator: "readonly",
        window: "readonly",
        document: "readonly",
        fetch: "readonly",
        localStorage: "readonly",
        AbortController: "readonly",
        URL: "readonly",
        FormData: "readonly",
        crypto: "readonly",
        setTimeout: "readonly",
        clearTimeout: "readonly",
        Intl: "readonly",
        Blob: "readonly",
        Event: "readonly",
        EventTarget: "readonly",
        CustomEvent: "readonly",
      },
    },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
);
