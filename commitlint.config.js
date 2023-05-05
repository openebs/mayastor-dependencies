module.exports = {
  extends: ['@commitlint/config-conventional'],
  rules: {
    'type-enum': [2, 'always', ['build', 'chore', 'ci', 'docs', 'feat', 'fix', 'perf', 'refactor', 'revert', 'style', 'test', 'example']],
  },
  defaultIgnores: false,
  ignores: [
      (message) => message.startsWith('chore(bors): merge pull request #'),
      (message) => message.startsWith('Merge pull request #'),
      (message) => message.startsWith('Merge #')
  ]
}
