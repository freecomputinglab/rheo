// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "undefined_var.typ", "undefined_variable", "│"
// @rheo:formats pdf
// Test file with undefined variable error

= Undefined Variable Test

// This will cause an error: undefined_variable doesn't exist
The value is: #undefined_variable
