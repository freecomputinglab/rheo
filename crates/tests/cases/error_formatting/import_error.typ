// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "import_error.typ", "│"
// @rheo:formats pdf
// Test file with import error (missing file)

= Import Error Test

// Try to include a file that doesn't exist
#include "nonexistent_file.typ"

Some content
