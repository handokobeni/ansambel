export default {
  extends: ['@commitlint/config-conventional'],
  rules: {
    'type-enum': [
      2,
      'always',
      [
        'feat',
        'fix',
        'chore',
        'docs',
        'ci',
        'test',
        'refactor',
        'perf',
        'style',
        'build',
        'revert',
      ],
    ],
    'subject-case': [0],
    'body-max-line-length': [1, 'always', 100],
  },
};
