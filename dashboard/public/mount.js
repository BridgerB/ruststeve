// Binds steve's Babylon viewer.js onto each bot card's <canvas class="v3d" data-vport>.
// The cards appear after the dashboard hydrates + its first data poll, so we watch the
// DOM and mount any canvas we haven't seen yet (each bot streams on its own SSE port).
import { mountViewer } from '/viewer.js';

const mounted = new WeakSet();

const mountAll = () => {
	document.querySelectorAll('canvas.v3d[data-vport]').forEach((canvas) => {
		if (mounted.has(canvas)) return;
		mounted.add(canvas);
		try {
			mountViewer(canvas, `http://localhost:${canvas.dataset.vport}`, {
				workerUrl: '/worker.js',
			});
		} catch (e) {
			console.error('[mount] viewer failed for :' + canvas.dataset.vport, e);
		}
	});
};

new MutationObserver(mountAll).observe(document.documentElement, {
	childList: true,
	subtree: true,
});
window.addEventListener('load', mountAll);
setTimeout(mountAll, 800);
