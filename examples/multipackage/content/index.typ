#import "../rheo-slides/rheo-slides.typ": slide, template
#import "../my-tooltip/my-tooltip.typ": tooltip, tooltip-content, tooltip-modal

#show: template

#slide[
  Multipackage Demo
]

This example combines slides and tooltips in one project.

#tooltip(placement: "bottom-start")[
  #tooltip-modal[*Details*]
  #tooltip-content[Hover to reveal.]
]
