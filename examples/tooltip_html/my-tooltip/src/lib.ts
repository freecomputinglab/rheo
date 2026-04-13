/// <reference types="vite/client" />

import { LitElement, html, css, unsafeCSS } from "lit";
import tippy, { type Instance, type Placement } from "tippy.js";
import tippyCss from "tippy.js/dist/tippy.css?raw";

const PLACEMENTS = new Set<string>([
  "top", "top-start", "top-end",
  "bottom", "bottom-start", "bottom-end",
  "left", "left-start", "left-end",
  "right", "right-start", "right-end",
  "auto", "auto-start", "auto-end",
]);

function isPlacement(s: string): s is Placement {
  return PLACEMENTS.has(s);
}

class TooltipElement extends LitElement {
  static properties = {
    placement: { type: String },
  };

  static styles = [
    css`
      :host {
        display: inline-block;
      }
      .content {
        cursor: pointer;
      }
    `,
    unsafeCSS(tippyCss)
  ];

  placement = "top";
  _tippy: Instance | null = null;

  firstUpdated() {
    const anchor = this.renderRoot.querySelector<HTMLSpanElement>(".content");
    const modalSlot = this.renderRoot.querySelector<HTMLSlotElement>('slot[name="modal"]');
    if (!anchor || !modalSlot) return;

    const modalNodes = modalSlot.assignedElements();
    if (modalNodes.length === 0) return;

    const container = document.createElement("div");
    for (const node of modalNodes) {
      container.appendChild(node.cloneNode(true));
    }

    const placement = isPlacement(this.placement) ? this.placement : "auto";

    this._tippy = tippy(anchor, {
      content: container.innerHTML,
      allowHTML: true,
      placement,
      interactive: true,
      appendTo: "parent",
    });
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this._tippy?.destroy();
  }

  render() {
    return html`
      <span class="content">
        <slot name="content"></slot>
      </span>
      <span hidden>
        <slot name="modal"></slot>
      </span>
    `;
  }
}

class TooltipModal extends HTMLElement {
  connectedCallback() {
    this.slot = "modal";
  }
}

class TooltipContent extends HTMLElement {
  connectedCallback() {
    this.slot = "content";
  }
}

customElements.define("my-tooltip", TooltipElement);
customElements.define("my-tooltip-modal", TooltipModal);
customElements.define("my-tooltip-content", TooltipContent);
