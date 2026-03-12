// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "multiple_errors.typ", "│"
// @rheo:formats pdf
// Test file with multiple errors

= Multiple Errors Test

// First error: undefined variable
#let x = undefined_var_one

// Second error: type mismatch
#let y = 5 + "string"

// Third error: undefined variable again
The value is: #undefined_var_two
