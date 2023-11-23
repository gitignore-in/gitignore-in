# gitignore.in

[![codecov](https://codecov.io/gh/gitignore-in/gitignore-in/graph/badge.svg?token=1XOPN2U21W)](https://codecov.io/gh/gitignore-in/gitignore-in)

Website: https://www.gitignore.in/

## Motivation

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

<img width="735" alt="concept" src="https://github.com/gitignore-in/gitignore-in/assets/2596972/68cdeb1e-2b2b-4b01-9c2b-fc4286027e39">

`gitignore.in` is a Command Line Interface (CLI) tool that generates `.gitignore` from a template.

## Usage

Following command generates `.gitignore.in` file and `.gitignore` file.

```bash
$ gitignore.in
```

In the `.gitignore.in` file, write the template you want to use like shell script.

```bash
$ cat .gitignore.in
# This is a comment
gibo dump Linux
gibo dump macOS
gibo dump Windows
gibo dump Python
echo '.coverage'
echo '.env'
```

When you run `gitignore.in` again, `.gitignore` will be updated.

## Installation

### Binary Releases

Download the binary from the [releases page](https://github.com/gitignore-in/gitignore-in/releases).
And place it in a directory that is in the PATH.

### Homebrew installation

```bash
$ brew tap gitignore-in/gitignore-in
$ brew install gitignore-in
```

## Manual Installation

```bash
$ git clone
$ cd gitignore-in
$ cargo install --path .
```

## License

MIT License
