(() => {
  const root = new URL('.', document.currentScript.src);
  const input = document.getElementById('search');
  const results = document.getElementById('search-results');
  const status = document.getElementById('search-status');
  let entries;
  let failed = false;
  const update = () => {
    results.replaceChildren();
    const query = input.value.trim().toLowerCase();
    results.hidden = !query;
    if (!query) { status.textContent = ''; return; }
    if (failed) { status.textContent = 'Search unavailable. Browse the module links below.'; return; }
    if (!entries) { status.textContent = 'Loading search…'; return; }
    const matches = entries.filter(entry => `${entry.module} ${entry.name} ${entry.signature} ${entry.summary}`.toLowerCase().includes(query));
    status.textContent = `${matches.length} result${matches.length === 1 ? '' : 's'}`;
    for (const entry of matches.slice(0, 40)) {
      const link = document.createElement('a');
      link.href = new URL(entry.url, root).href;
      link.textContent = entry.name;
      const module = document.createElement('small');
      module.textContent = entry.module;
      link.append(module);
      results.append(link);
    }
  };
  input.addEventListener('input', update);
  fetch(new URL('search.json', root)).then(response => {
    if (!response.ok) throw new Error('Search unavailable');
    return response.json();
  }).then(data => { entries = data; update(); }).catch(() => {
    failed = true;
    update();
  });
})();
