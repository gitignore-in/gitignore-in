# gitignore.in

[![codecov](https://codecov.io/gh/gitignore-in/gitignore-in/graph/badge.svg?token=1XOPN2U21W)](https://codecov.io/gh/gitignore-in/gitignore-in)

## Notice

This project is still under development.
See https://github.com/gitignore-in/legacy-gitignore-in-script for the old version.

## Problem

The method of generating `.gitignore` from a template is already widespread.
For example, there are the following methods.

- gibo
- gitignore.io
- .gitignore generated when creating a repository on GitHub

However, the `.gitignore` file generated by these methods is not updated over time.
The result of `gibo dump Python` executed in 2018 and the result of `gibo dump Python` executed in 2023 will be different.
In order for the project to use the latest `.gitignore`, you need to update `.gitignore` regularly.
However, this is a troublesome task that can cause mistakes.

## Solution

`gitignore.in` is a Command Line Interface (CLI) tool that generates `.gitignore` from a template.

## License

MIT License
