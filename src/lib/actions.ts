import { animate } from 'motion';
import gsap from 'gsap';

/** Motion: press feedback (scale down/up) for buttons. */
export function pressable(node: HTMLElement) {
  let pressed = false;
  const down = () => {
    pressed = true;
    animate(node, { scale: 0.96 }, { duration: 0.08 });
  };
  const up = () => {
    if (!pressed) return;
    pressed = false;
    animate(node, { scale: 1 }, { duration: 0.2 });
  };
  node.addEventListener('pointerdown', down);
  node.addEventListener('pointerup', up);
  node.addEventListener('pointerleave', up);
  return {
    destroy() {
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointerup', up);
      node.removeEventListener('pointerleave', up);
    }
  };
}

/** Motion: entrance animation - fade + rise. */
export function riseIn(node: HTMLElement, opts?: { delay?: number }) {
  animate(
    node,
    { opacity: [0, 1], y: [12, 0] },
    { duration: 0.45, delay: opts?.delay ?? 0, ease: [0.22, 1, 0.36, 1] }
  );
}

/** GSAP: badge pulse whenever the tracked value changes. */
export function pulse(node: HTMLElement, value: string) {
  let last = value;
  return {
    update(v: string) {
      if (v === last) return;
      last = v;
      gsap.fromTo(
        node,
        { scale: 1 },
        { scale: 1.16, duration: 0.12, yoyo: true, repeat: 1, ease: 'power2.out' }
      );
    }
  };
}

/** GSAP: view transition (use with `transition:` directive). */
export function viewIn(node: HTMLElement) {
  gsap.fromTo(
    node,
    { opacity: 0, y: 16 },
    { opacity: 1, y: 0, duration: 0.4, ease: 'power3.out', clearProps: 'transform' }
  );
  return { duration: 400 };
}
