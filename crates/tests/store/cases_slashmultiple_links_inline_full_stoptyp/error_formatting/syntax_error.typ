// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "syntax_error.typ", "│"
// @rheo:formats pdf
// Test file with syntax error (unclosed delimiter)

= Syntax Error Test

#let items = [
  Item 1,
  Item 2,
  Item 3
// Missing closing bracket ]

Content follows
