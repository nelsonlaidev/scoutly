# Scoutly

This package installs the Scoutly CLI for macOS, Linux, or Windows from the
matching [GitHub release](https://github.com/nelsonlaidev/scoutly/releases).

```sh
npm install --global @nelsonlaidev/scoutly
scoutly https://example.com
```

Prereleases are published to the opt-in `beta` dist-tag:

```sh
npm install --global @nelsonlaidev/scoutly@beta
```

The installer verifies the downloaded archive against the release's SHA-256
checksums before installing the executable.

See the [Scoutly repository](https://github.com/nelsonlaidev/scoutly) for CLI
usage and configuration.
