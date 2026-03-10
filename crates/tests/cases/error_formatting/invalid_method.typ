// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "invalid_method.typ", "│"
// @rheo:formats pdf
// Test file with invalid method call

= Invalid Method Test

// Try to call a method that doesn't exist on strings
#let text = "hello"
#let result = text.nonexistent_method()

Some content
