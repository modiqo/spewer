# CP27 evidence: Spewer field note on GitHub Pages

Status: **Complete**

CP27 publishes a static, crawler-readable essay that explains why Spewer exists and how the
implemented delegation boundary works.

## Publication contract

- The title states the token-allocation claim in twelve words or fewer.
- The essay separates current Spewer 0.2 behavior from future harness adapters.
- The article credits Calvin French-Owen and links his original post.
- The architecture visual shows a pluggable frontier, Spewer, and pluggable workers.
- The Pareto visual identifies itself as illustrative instead of presenting invented benchmarks.
- HTML, metadata, Atom, robots, and sitemap content remain readable without JavaScript.
- GitHub Actions deploys only the assembled static site to GitHub Pages.

## Exit gate

The checkpoint closes after local markup, link, responsive-layout, workflow, and desk checks pass.
The live HTTPS page must return the article title, assets, metadata, and source links.

## Evidence

- Commit `87a75edb51edade5c1844d84b75a24b1c241203e` added the article and the pinned
  GitHub Pages workflow.
- [GitHub Actions run 33295986578](https://github.com/modiqo/spewer/actions/runs/33295986578)
  assembled and deployed the static artifact successfully.
- [The live article](https://modiqo.github.io/spewer/) returns HTTP 200 over HTTPS.
- The stylesheet, architecture image, Atom feed, sitemap, and robots file each return HTTP 200.
- Browser checks found all thirteen article sections, no horizontal overflow at desktop or mobile
  widths, and no site-authored executable JavaScript.
- The live document exposes the title, canonical URL, BlogPosting data, Calvin French-Owen source,
  current Codex integration boundary, and illustrative Pareto disclaimer.
- `actionlint`, XML validation, Desk lint, documentation limits, Rust source limits, panic audits,
  and `git diff --check` passed locally.

The workflow reported an upstream Node.js deprecation notice from `actions/configure-pages@v5`.
It did not affect assembly or deployment.
