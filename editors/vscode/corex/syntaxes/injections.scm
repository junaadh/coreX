; Conservative doc-comment injection for markdown-like editor rendering.
((doc_comment) @injection.content
  (#set! injection.language "markdown"))

((inner_doc_comment) @injection.content
  (#set! injection.language "markdown"))
