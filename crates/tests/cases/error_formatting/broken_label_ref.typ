// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "label", "does not exist", "nonexistent-label", "│"
// @rheo:formats html
// Test error message for broken cross-document label reference

= Broken Label Reference Test

This document contains a link to a non-existent label.

See #link(<nonexistent-label>)[broken link] for more information.
