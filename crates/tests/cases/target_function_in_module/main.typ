// @rheo:test
// @rheo:formats html,pdf,epub
// @rheo:description Verifies target() works in imported modules

#import "lib/format_helper.typ": get_format, format_specific_content

= Target Function in Module

== Main File
#context [Main: *#target()*]

== Imported Module
#context [Module returns: *#get_format()*]

== Module Conditional
#format_specific_content()
