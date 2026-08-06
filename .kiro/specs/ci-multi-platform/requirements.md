# Requirements: CI Multi-Platform Build

## REQ-001: Windows Build
CI должен собирать бинарник под Windows (x86_64-pc-windows-msvc).

## REQ-002: Release Build
CI должен собирать в `--release` режиме.

## REQ-003: Binary Artifacts
CI должен загружать собранные бинарники как artifacts.

## REQ-004: Release Workflow
При создании git tag CI должен публиковать бинарники в GitHub Releases.
