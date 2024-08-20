# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.2.0] - 2024-08-19

### Added

- `base-url` option handles scenarios where the site will be served on a base
  path that isn't the root of the domain.

## [1.1.1] - 2024-08-12
 
### Fixed

- Use full URLs in the sitemap. Sitemaps will only be generated if the optional
  `domain` CLI arg is provided.

## [1.1.0] - 2024-08-12

### Added

- Generate a sitemap and serve it at the root of the site.

## [1.0.2] - 2024-07-15

### Changed

- Minor tweaks to documentation.

## [1.0.1] - 2024-07-15

### Added

- Basic documentation.

## [1.0.0] - 2024-07-15

### Added

- Initial release.
