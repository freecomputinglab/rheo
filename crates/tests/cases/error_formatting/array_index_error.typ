// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "array_index_error.typ", "│"
// @rheo:formats pdf
// Test file with array index error

= Array Index Error Test

// Create a small array
#let items = ("first", "second", "third")

// Try to access an index that doesn't exist
#items.at(10)

Some content
