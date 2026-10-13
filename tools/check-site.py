import html.parser
import re


class P(html.parser.HTMLParser):
    VOID = {'area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input',
            'link', 'meta', 'param', 'source', 'track', 'wbr'}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.stack = []
        self.errs = []

    def handle_starttag(self, tag, attrs):
        if tag not in self.VOID:
            self.stack.append((tag, self.getpos()))

    def handle_endtag(self, tag):
        if tag in self.VOID:
            return
        if not self.stack:
            self.errs.append(f'stray </{tag}>')
            return
        if self.stack[-1][0] != tag:
            self.errs.append(f'mismatch </{tag}> vs <{self.stack[-1][0]}>')
        else:
            self.stack.pop()


src = open(r'docs\index.html', encoding='utf-8').read()
p = P()
p.feed(src)
print('  unclosed:', [t for t, _ in p.stack] or 'none')
print('  errors  :', p.errs or 'none')
print('  feature cards:', len(re.findall(r'<div class="card">', src)))
print('  size:', len(src.encode()), 'bytes')
