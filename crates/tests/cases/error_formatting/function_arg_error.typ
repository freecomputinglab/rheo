// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "function_arg_error.typ", "│"
// @rheo:formats pdf
// Test file with function argument error

= Function Argument Error Test

// Define a function that requires two arguments
#let add_numbers(x, y) = x + y

// Call it with only one argument (missing required argument)
#add_numbers(5)

Some content
