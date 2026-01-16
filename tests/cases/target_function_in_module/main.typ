// @rheo:test
// @rheo:formats html,pdf,epub
// @rheo:description Verifies sys.inputs.rheo-target works in imported modules

#import "lib/format_helper.typ": rheo-target, get_format, format_specific_content

= Target Function in Module

== Main File
#context [Main: *#rheo-target()*]

== Imported Module
#context [Module returns: *#get_format()*]

== Module Conditional
#format_specific_content()
