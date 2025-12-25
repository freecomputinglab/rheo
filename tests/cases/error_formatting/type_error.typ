// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "type_error.typ", "│"
// @rheo:formats pdf
// Test file with type error
// This should trigger a Typst compilation error

= Type Error Test

#let x = 5
#let y = "hello"

// This will cause a type error: can't add number and string
#let result = x + y

Content: #result
