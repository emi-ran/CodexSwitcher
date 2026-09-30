(() => {
  const tooltip = document.createElement('div');
  tooltip.id = 'app-tooltip';
  tooltip.className = 'app-tooltip';
  tooltip.setAttribute('role', 'tooltip');
  tooltip.hidden = true;
  const heading = document.createElement('div');
  heading.className = 'app-tooltip-heading';
  const detail = document.createElement('div');
  detail.className = 'app-tooltip-detail';
  tooltip.append(heading, detail);
  document.body.appendChild(tooltip);

  let target = null;
  let showTimer;
  let hideTimer;

  function hide() {
    clearTimeout(showTimer);
    clearTimeout(hideTimer);
    tooltip.hidden = true;
    if (target) {
      const ids = (target.getAttribute('aria-describedby') || '').split(/\s+/).filter(id => id && id !== tooltip.id);
      if (ids.length) target.setAttribute('aria-describedby', ids.join(' '));
      else target.removeAttribute('aria-describedby');
    }
    target = null;
  }

  function position() {
    if (!target?.isConnected) return hide();
    const anchor = target.getBoundingClientRect();
    if (anchor.bottom < 0 || anchor.top > window.innerHeight || anchor.right < 0 || anchor.left > window.innerWidth) return hide();
    const box = tooltip.getBoundingClientRect();
    const left = Math.max(12, Math.min(anchor.left + (anchor.width - box.width) / 2, window.innerWidth - box.width - 12));
    const above = anchor.top - box.height - 8;
    const top = above >= 12 ? above : Math.min(anchor.bottom + 8, window.innerHeight - box.height - 12);
    tooltip.style.left = `${left}px`;
    tooltip.style.top = `${Math.max(12, top)}px`;
  }

  function show(element, immediate = false) {
    clearTimeout(hideTimer);
    if (element === target && (!immediate || !tooltip.hidden)) return;
    hide();
    target = element;
    const reveal = () => {
      if (!target?.isConnected) return hide();
      heading.textContent = target.dataset.tooltip;
      detail.textContent = target.dataset.tooltipDetail || '';
      detail.hidden = !detail.textContent;
      tooltip.hidden = false;
      const ids = new Set((target.getAttribute('aria-describedby') || '').split(/\s+/).filter(Boolean));
      ids.add(tooltip.id);
      target.setAttribute('aria-describedby', [...ids].join(' '));
      position();
    };
    if (immediate) reveal();
    else showTimer = setTimeout(reveal, 220);
  }

  const trigger = node => node instanceof Element ? node.closest('[data-tooltip]:not([data-tooltip=""])') : null;
  document.addEventListener('pointerover', event => {
    if (tooltip.contains(event.target)) return clearTimeout(hideTimer);
    const element = trigger(event.target);
    if (element) show(element);
  });
  document.addEventListener('pointerout', event => {
    if (target?.contains(event.relatedTarget) || tooltip.contains(event.relatedTarget)) return;
    if (target?.contains(event.target) || tooltip.contains(event.target)) hideTimer = setTimeout(hide, 120);
  });
  document.addEventListener('focusin', event => {
    const element = trigger(event.target);
    if (element) show(element, true);
    else hide();
  });
  document.addEventListener('focusout', hide);
  document.addEventListener('keydown', event => { if (event.key === 'Escape') hide(); });
  document.addEventListener('pointerdown', hide);
  document.addEventListener('scroll', () => { if (target) position(); }, true);
  window.addEventListener('resize', hide);
  window.addEventListener('blur', hide);
  window.hideAppTooltip = hide;
})();
