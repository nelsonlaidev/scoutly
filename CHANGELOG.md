# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0](https://github.com/nelsonlaidev/scoutly/compare/v0.1.1..v0.2.0) - 2025-11-20

### Features

- Add URL scheme validation - ([53d9bf8](https://github.com/nelsonlaidev/scoutly/commit/53d9bf888848170290b897b229075e2ac287ffee))
- Add content-type validation - ([9faf08a](https://github.com/nelsonlaidev/scoutly/commit/9faf08ae880f29ed049fb0be6c1eee6f3db8e5a2))
- Replace eprintln! with structured logging - ([4e15269](https://github.com/nelsonlaidev/scoutly/commit/4e15269133b3e162c501928373a035347b5d98a7))
- Add rate limiting configuration structure - ([6659a69](https://github.com/nelsonlaidev/scoutly/commit/6659a69c357e405b012279aefb3c3e32ad1afe6d))
- Implement rate limiting with governor - ([0cbff74](https://github.com/nelsonlaidev/scoutly/commit/0cbff747c891d4134b2d8269fc712d9595c0bf0f))
- Concurrent crawling - ([39209ab](https://github.com/nelsonlaidev/scoutly/commit/39209abb19c7a5458c7caf7fee692a076217d0ee))
- Expose more CLI options - ([600d10a](https://github.com/nelsonlaidev/scoutly/commit/600d10a1728d2e1eaef89c697ec0e92208c69d5c))
- Add robots.txt support to web crawler (#3) - ([e334454](https://github.com/nelsonlaidev/scoutly/commit/e334454799e190b47677056349f64608ded4b315))
- Add progress bar (#4) - ([ccf810d](https://github.com/nelsonlaidev/scoutly/commit/ccf810d3f7a02ee0a68a7c79c28c8ca70bfac24d))
- Build cross-platform config file parser (#5) - ([72a30f6](https://github.com/nelsonlaidev/scoutly/commit/72a30f6fc049df338457864fc7c128ed7b88d031))
- Add open graph meta tag validation (#6) - ([06721b5](https://github.com/nelsonlaidev/scoutly/commit/06721b5acb92950fdca4d04015f5c73ce93a2038))

### Bug Fixes

- No pontential panic in selector parsing - ([624ad3c](https://github.com/nelsonlaidev/scoutly/commit/624ad3c975ae9a2454532a468bd2bf6568d2e664))
- Max_pages off-by-one issue - ([01d72a7](https://github.com/nelsonlaidev/scoutly/commit/01d72a76db1a5e07321f401f3d17ce1bd0a060f9))

### Chores

- New deps - ([13cdb8e](https://github.com/nelsonlaidev/scoutly/commit/13cdb8e3974e30f2aa941a05be70c71878bd0ba8))
- Try to build for aarch64-pc-windows-msvc - ([171e51d](https://github.com/nelsonlaidev/scoutly/commit/171e51d99afed8306d17a4358ed9a2b19ffb24de))
- Update cliff config - ([fd44a79](https://github.com/nelsonlaidev/scoutly/commit/fd44a79e8c7863adb8e40dbae095e792ac8b1f60))
- Update cliff config - ([672921a](https://github.com/nelsonlaidev/scoutly/commit/672921a3dd50cc3815f590328a7093979fa053c2))
- Don't build for aarch64-pc-windows-msvc - ([c2a4d51](https://github.com/nelsonlaidev/scoutly/commit/c2a4d51018964a54292abfdb985cea04d499d9b9))

### Performance

- Optimize URL parsing by parsing once and reusing - ([59a1f5f](https://github.com/nelsonlaidev/scoutly/commit/59a1f5fcbd4f50321adb631841182f8fa94c841f))

### Refactor

- Link extraction to single DOM pass - ([a3cc9a3](https://github.com/nelsonlaidev/scoutly/commit/a3cc9a3c6e98bbd6e9643c7e8b47879d69fa6e91))
- Use let-chains and idiomatic empty checks - ([04259b0](https://github.com/nelsonlaidev/scoutly/commit/04259b0b722816a2aaeda5d093c2ac3b6e40d780))

### Testing

- Add tests for content-type validation, rate limiting, concurrency - ([acef227](https://github.com/nelsonlaidev/scoutly/commit/acef227a50cb60e5feb2a220c6110d07c9d84b67))
- Increase test coverage (#7) - ([6cc3594](https://github.com/nelsonlaidev/scoutly/commit/6cc3594bbfb3320415261112e83f3c47728859eb))

## [0.1.1](https://github.com/nelsonlaidev/scoutly/compare/v0.1.0..v0.1.1) - 2025-11-09

### Bug Fixes

- Should compare port when checking external url - ([7533344](https://github.com/nelsonlaidev/scoutly/commit/75333448888fb944e5307cf4660841db43073af2))

### Testing

- Add basic cli test - ([19efafc](https://github.com/nelsonlaidev/scoutly/commit/19efafcec5efa2147708c97e90598579bac1e145))
- Add tests for max_depth, follow_external params - ([ba9b4ef](https://github.com/nelsonlaidev/scoutly/commit/ba9b4ef7a6f4fc5f06d4779f5d5b4de16ab062a5))
- Platform-specific pattern in cli_test - ([a02985e](https://github.com/nelsonlaidev/scoutly/commit/a02985e0daf9e5d3e7e098c9a33db8a2cf18539d))

## [0.1.0](https://github.com/nelsonlaidev/scoutly/tree/v0.1.0) - 2025-11-06

### Bug Fixes

- Don't analyze SEO for non-html pages - ([ec93116](https://github.com/nelsonlaidev/scoutly/commit/ec9311641c92079ea9638eb4ebac2ea548b2863d))

### Chores

- Initial commit - ([5122efb](https://github.com/nelsonlaidev/scoutly/commit/5122efb3d317a75cabdda955f48dd0344db11654))
- Add lefthook - ([f8785ff](https://github.com/nelsonlaidev/scoutly/commit/f8785ff8b8a212e65226c613f68671b54d15fce1))
- Update lefthook config - ([c95b68f](https://github.com/nelsonlaidev/scoutly/commit/c95b68f2d8e49fcbe9ae304fffdc7b42c42d4879))
- Specify rust version - ([0715f69](https://github.com/nelsonlaidev/scoutly/commit/0715f69ec3548a75a06523b4c518612e742f7a92))
- Add CODEOWNERS and FUNDING.yml - ([7f431af](https://github.com/nelsonlaidev/scoutly/commit/7f431af1952cf4fdf302fb99babfedf2b6023903))
- Update Cargo.toml - ([ee8f5f0](https://github.com/nelsonlaidev/scoutly/commit/ee8f5f08e86e08bfb114145903b5d9a78f470f80))
- Update codecov config - ([b8d9a95](https://github.com/nelsonlaidev/scoutly/commit/b8d9a95a8559f984741647bc4f8fdcd720286824))
- Add release workflow - ([2f51608](https://github.com/nelsonlaidev/scoutly/commit/2f516087a0d045eceab641aff0207771863688cf))
- Update cargo-dist config - ([3b397e2](https://github.com/nelsonlaidev/scoutly/commit/3b397e26d5029e689c7250455540058fe92004e6))
- Alpha 1 - ([a5b7da1](https://github.com/nelsonlaidev/scoutly/commit/a5b7da1ad5783b3b0b5b04c4c98199c082e8b410))
- Add cliff config - ([993b060](https://github.com/nelsonlaidev/scoutly/commit/993b060f5e1073c96f3f55380342b9eaa00ded20))
- Add rust-toolchain - ([19ea00b](https://github.com/nelsonlaidev/scoutly/commit/19ea00b2f5358b83cdc4e349d3cf0643462c1982))
- Update rust-toolchain config - ([68d8317](https://github.com/nelsonlaidev/scoutly/commit/68d8317e41e35f6c76667e9b66c7c5496af82125))
- Update reqwest dependencies to include rustls-tls feature - ([fde1e79](https://github.com/nelsonlaidev/scoutly/commit/fde1e7900206f0282ece915fa0c2c09f9dad313b))
- Temporarily skip aarch64-pc-windows-msvc builds - ([ea00889](https://github.com/nelsonlaidev/scoutly/commit/ea008894e16946b421a33a68ec31f8c2ccca3ad3))
- Version 0.1.0 - ([6a035ca](https://github.com/nelsonlaidev/scoutly/commit/6a035ca4fbd5cbdf010c8760401c87a7d660a11a))
- Add npm package name to dist-workspace - ([36ddb41](https://github.com/nelsonlaidev/scoutly/commit/36ddb41f304314e1416e566e4c691bf89cac0fc6))
- Publish to npm - ([570f726](https://github.com/nelsonlaidev/scoutly/commit/570f726de3db590e1dd7a1b03b84b08efc6c09e1))
- _(release)_ Prepare for v0.1.0 - ([638755a](https://github.com/nelsonlaidev/scoutly/commit/638755adfdf0b5ed851b1ab7faea94211e355171))

### Continuous Integration

- Add quality checks - ([f28fddf](https://github.com/nelsonlaidev/scoutly/commit/f28fddf4d1447fb53342609f5fc4b11e1c8b1234))
- Cross platform ci, check formatting - ([e17ced6](https://github.com/nelsonlaidev/scoutly/commit/e17ced626b238d14d54c7b17b79dff4b28f254f1))
- Update ci - ([073adc4](https://github.com/nelsonlaidev/scoutly/commit/073adc42848bd7e330cd2aaa05c96b96c77a8f18))
- Add codecov - ([1f44eab](https://github.com/nelsonlaidev/scoutly/commit/1f44eab573c9903f710203cd49d26853c6f78fac))
- Make the code consistent - ([8d7ed2c](https://github.com/nelsonlaidev/scoutly/commit/8d7ed2c8ab20afac664ae9f904fa2053c1e6da84))
- Specify permissions - ([e07b75e](https://github.com/nelsonlaidev/scoutly/commit/e07b75eed523d063404cf4551e6b95f64301c306))
- Update ci - ([0f66dfc](https://github.com/nelsonlaidev/scoutly/commit/0f66dfc8180dacb2a89c87fc4364b309322f5e3c))

### Documentation

- Update readme - ([cd352fd](https://github.com/nelsonlaidev/scoutly/commit/cd352fdbf13db1fccc661699e2a264e1f1e11d34))
- Update readme - ([fe0820e](https://github.com/nelsonlaidev/scoutly/commit/fe0820e7ef5035b89e7a5387db61b84e1414347c))

### Refactor

- Format code - ([409d55d](https://github.com/nelsonlaidev/scoutly/commit/409d55d5628a7dce7b9a34f73f940ecdda5d87e6))
- Fix clippy warnings - ([dfa413c](https://github.com/nelsonlaidev/scoutly/commit/dfa413c0fb04264fff5a7d1317b70f294ef06f97))
- Format code - ([a088bab](https://github.com/nelsonlaidev/scoutly/commit/a088bab578d02175e718d033935d73a0dc9193f3))

### Testing

- Coverage (#1) - ([979e6eb](https://github.com/nelsonlaidev/scoutly/commit/979e6eb0dba070dd7a7b85f73d38a92f0873bf88))
- More coverage (#2) - ([3290f46](https://github.com/nelsonlaidev/scoutly/commit/3290f465ffc706eada1c44a34b073a11daf6960b))


