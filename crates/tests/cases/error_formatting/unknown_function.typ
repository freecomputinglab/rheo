// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "unknown_function.typ", "│"
// @rheo:formats pdf
// Test file with unknown function call

= Unknown Function Test

// Call a function that doesn't exist
#nonexistent_function("arg1", "arg2")

Some content here
