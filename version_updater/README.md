# Version Updater

A fast Fortran utility for automatically updating version numbers across multiple file types in your project.

## Overview

Version Updater recursively scans your project directory and updates version strings in configuration files. This tool is particularly useful for maintaining consistent version information across a project with multiple components.

## Features

- Updates version numbers in multiple file types:
  - `tauri.conf.json`
  - `Cargo.toml`
  - `Cargo.lock` (balatro-mod-manager package)
  - `package.json`
  - `packaging/flatpak/io.balatro.ModManager.metainfo.xml`
- Intelligent version handling (removes `v` prefix for certain files)
- Preserves file formatting and structure
- Excludes common directories like `.git`, `node_modules`, etc.
- Fast performance with optional OpenMP multithreading

**Note:** Svelte files no longer need version updates as the frontend now fetches the version dynamically from the backend via `get_app_version`.

## Requirements

- Fortran compiler (gfortran recommended)
- Fortran Package Manager (fpm)
- OpenMP support (optional, for improved performance)

## Building

## Build with fpm

`fpm build --flag "-fopenmp" --profile release`

Note: If your compiler doesn't support OpenMP, you can build without it:
`fpm build --profile release`

## Usage

### Basic Usage

Update version numbers in a given project directory:

`fpm run -- v2.0.3 /path/to/project`
