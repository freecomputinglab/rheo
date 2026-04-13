#import "../my-tooltip/dist/my-tooltip.typ": tooltip, tooltip-content, tooltip-modal

#tooltip(placement: "bottom-start")[
  #tooltip-modal[*Hello world!*]
  #tooltip-content[Hover over me.]
]