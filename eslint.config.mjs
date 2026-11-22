// ESLint lints and formats (no Prettier): @antfu/eslint-config with its formatters.
import antfu from '@antfu/eslint-config'

export default antfu(
  {
    type: 'app',
    typescript: true,
    formatters: true,
    ignores: [
      '**/generated',
      'docs/design/raw',
      '**/.nsh',
      '**/target',
      '**/dist',
    ],
  },
  {
    // Cargo manifests group `x.workspace = true` keys without blank lines between them.
    files: ['**/*.toml'],
    rules: {
      'toml/padding-line-between-pairs': 'off',
    },
  },
)
