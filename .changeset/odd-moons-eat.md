---
"@biomejs/biome": patch
---

Fixed [#11893](https://github.com/biomejs/biome/issues/11893): [`noDeprecatedImports`](https://biomejs.dev/linter/rules/no-deprecated-imports/) reports overloaded functions only when all public signatures are deprecated, allowing imports of functions with a non-deprecated overload.
