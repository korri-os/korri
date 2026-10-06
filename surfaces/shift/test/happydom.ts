import { GlobalRegistrator } from "@happy-dom/global-registrator"

GlobalRegistrator.register()

/**
 * Web Animations: cancel() rejects `finished` and then sets the promise's
 * [[PromiseIsHandled]] slot to true, so a cancelled animation is not an
 * unhandled rejection (https://drafts.csswg.org/web-animations-1/#canceling-an-animation-section).
 * happy-dom 20 skips that step, and motion cancels animations on unmount.
 * Mark the promise handled before happy-dom rejects it.
 */
const cancelAnimation = Animation.prototype.cancel
Animation.prototype.cancel = function cancel(this: Animation) {
  this.finished.catch(() => undefined)
  cancelAnimation.call(this)
}
