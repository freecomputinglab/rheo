#import "@rheo/slides:0.1.0": slide, template
#import "@rheo/tooltip:0.1.0": tooltip, tooltip-content, tooltip-modal

#show: template

#slide[
  Multipackage Demo
]

This example combines slides and tooltips in one project.

#slide[
  == Here is a tooltip
  #tooltip(placement: "bottom-start")[
    #tooltip-modal[*Details*]
    #tooltip-content[Hover to reveal.]
  ]
]
