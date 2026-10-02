import { animate } from 'motion';

/** Motion：按压反馈（缩放），用于门户按钮。 */
export function pressable(node: HTMLElement) {
	let pressed = false;
	const down = () => {
		pressed = true;
		animate(node, { scale: 0.97 }, { duration: 0.08 });
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
