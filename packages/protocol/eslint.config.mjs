import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/**", "node_modules/**", "coverage/**"] },

  // Type aware linting needs a program, and the program is the TypeScript one. The two config
  // files and the generator are plain ESM that TypeScript never compiles, so they are linted
  // without it rather than forced into the project service, which would mean turning on allowJs
  // and typechecking build scripts as if they were library code.
  {
    files: ["**/*.ts"],
    extends: [...tseslint.configs.strictTypeChecked, ...tseslint.configs.stylisticTypeChecked],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/no-non-null-assertion": "error",
      "@typescript-eslint/consistent-type-imports": ["error", { prefer: "type-imports" }],
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
      // Bit twiddling in the codecs is the whole job and every operand there is a number by
      // construction. What this rule is actually worth catching is an accidental string
      // concatenation, which is exactly the mistake a byte for byte mirror of a Solidity
      // library cannot afford to make.
      "@typescript-eslint/restrict-plus-operands": "error",
    },
  },

  {
    files: ["**/*.mjs"],
    extends: [tseslint.configs.base],
    rules: {},
  },

  {
    // Generated from the Foundry build. Lint has nothing useful to say about a JSON blob, and
    // every rule it could fire would be a rule aimed at the generator instead.
    files: ["src/abi/**/*.ts"],
    rules: {
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-assignment": "off",
    },
  },

  {
    // Tests say the quiet part out loud: fixed vectors, magic offsets, and deliberate nonsense
    // handed to a parser to watch it refuse. Rules written for library code get in the way.
    files: ["test/**/*.ts"],
    rules: {
      "@typescript-eslint/no-magic-numbers": "off",
      "@typescript-eslint/no-unsafe-assignment": "off",
      "@typescript-eslint/no-unsafe-member-access": "off",
      "@typescript-eslint/no-unsafe-call": "off",
      "@typescript-eslint/no-unsafe-argument": "off",
      "@typescript-eslint/no-non-null-assertion": "off",
    },
  },
);
