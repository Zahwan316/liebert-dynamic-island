# Contributing to liebert-dynamic-island

Thanks for your interest in contributing! Every contribution, whether it's a bug report, a suggestion, a fix, or a new feature, is welcome.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How Can I Contribute?](#how-can-i-contribute)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Commit Messages](#commit-messages)
- [Pull Request Guidelines](#pull-request-guidelines)
- [Reporting Bugs](#reporting-bugs)
- [Suggesting Features](#suggesting-features)
- [Questions](#questions)

## Code of Conduct

Be respectful and constructive. Harassment, discrimination, or hostile behavior will not be tolerated. Keep discussions focused on the project and treat everyone with courtesy.

## How Can I Contribute?

- **Report bugs** by opening an issue with clear steps to reproduce.
- **Suggest features or improvements** by opening an issue describing the use case.
- **Improve documentation**, such as the README, usage instructions, or translations.
- **Submit code changes** through a pull request.

## Getting Started

1. **Fork** the repository on GitHub.
2. **Clone** your fork locally:

   ```bash
   git clone https://github.com/<your-username>/liebert-dynamic-island.git
   cd liebert-dynamic-island
   ```

3. **Add the upstream remote** so you can keep your fork in sync:

   ```bash
   git remote add upstream https://github.com/Zahwan316/liebert-dynamic-island.git
   ```

4. **Install dependencies** and set up the project as described in the README.

## Development Workflow

1. Sync your fork with the latest changes before starting:

   ```bash
   git checkout main
   git pull upstream main
   ```

2. Create a new branch for your work. Use a descriptive name:

   ```bash
   git checkout -b fix/high-cpu-usage
   # or
   git checkout -b feature/custom-theme
   ```

3. Make your changes. Keep each pull request focused on a single change.
4. Test your changes thoroughly on Windows, since this application targets Windows systems.
5. Push your branch and open a pull request against `main`.

## Commit Messages

Write clear, concise commit messages in the imperative mood, for example:

- `Fix memory leak when idle`
- `Add option to change island position`
- `Update README with build instructions`

Keep the subject line under 72 characters. Add more detail in the body if the reason for the change isn't obvious.

## Pull Request Guidelines

Before submitting a pull request, please make sure that:

- [ ] Your code builds and runs without errors.
- [ ] You have tested the change on your own system.
- [ ] Your change is limited to what the PR describes. Avoid unrelated formatting or refactoring.
- [ ] You have updated the documentation if your change affects usage or configuration.
- [ ] The PR description explains **what** changed and **why**. Link related issues using `Closes #123` or `Fixes #123`.

Maintainers may request changes before merging. This is normal and part of the review process. Please be patient and open to feedback.

## Reporting Bugs

When opening a bug report, please include:

- A clear and descriptive title.
- Steps to reproduce the problem.
- What you expected to happen and what actually happened.
- Your Windows version and hardware details, if relevant.
- Screenshots or logs, if available.
- The version of liebert-dynamic-island you are using.

Before opening a new issue, please search existing issues to check whether it has already been reported.

## Suggesting Features

Feature requests are welcome. When suggesting a feature, please describe:

- The problem you are trying to solve.
- How you imagine the feature working.
- Any alternatives you have considered.

## Questions

If you have a question about using or contributing to the project, feel free to open an issue with the `question` label instead of a bug report.

---

Thank you for helping make liebert-dynamic-island better!
