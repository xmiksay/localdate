import js from '@eslint/js'
import globals from 'globals'
import pluginVue from 'eslint-plugin-vue'
import tseslint from 'typescript-eslint'
import prettier from 'eslint-config-prettier'

export default tseslint.config(
  { ignores: ['dist', 'dev-dist', 'node_modules'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs['flat/recommended'],
  {
    files: ['**/*.{ts,vue}'],
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
      parserOptions: { parser: tseslint.parser },
    },
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
      'no-restricted-imports': [
        'error',
        {
          paths: [
            {
              name: 'vue-i18n',
              importNames: ['useI18n'],
              message: "Use useT() from '@/i18n/typed' so message keys are type-checked.",
            },
          ],
        },
      ],
    },
  },
  { files: ['src/i18n/typed.ts'], rules: { 'no-restricted-imports': 'off' } },
  prettier,
)
