Based on the published `mustache` 0.9.0 crate (MIT license in LICENSE).
The only library source change is the path lookup in `src/template.rs`.
Node-RED uses mustache.js, which allows numeric components to index arrays.
The Template node test exercises indexing and ordinary section iteration together.
