// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "invalid_field.typ", "│"
// @rheo:formats pdf
// Test file with invalid field access

= Invalid Field Access Test

// Create a dictionary and try to access non-existent field
#let person = (
  name: "Alice",
  age: 30
)

// Try to access a field that doesn't exist
#person.nonexistent_field

Some content
