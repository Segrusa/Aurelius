import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/data/markdown_paths.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('detects Markdown paths and links', () {
    expect(isMarkdownPath('docs/plan.md'), isTrue);
    expect(isMarkdownPath('README.MARKDOWN#top'), isTrue);
    expect(isMarkdownPath('https://example.com/a.md'), isFalse);
    expect(isMarkdownPath('notes.txt'), isFalse);
  });

  test('resolves relative links against the open file', () {
    expect(resolveMarkdownPath('b.md', from: 'docs/a.md'), 'docs/b.md');
    expect(resolveMarkdownPath('../README.md#x', from: 'docs/a.md'), 'README.md');
    expect(resolveMarkdownPath('./sub/c.md', from: 'a.md'), 'sub/c.md');
    expect(resolveMarkdownPath('C:\\work\\x.md', from: 'docs/a.md'), 'C:/work/x.md');
    expect(resolveMarkdownPath('/home/me/x.md'), '/home/me/x.md');
  });

  test('links inline code paths but not fenced code', () {
    expect(linkMarkdownPaths('See `docs/a.md` now'), 'See [`docs/a.md`](docs/a.md) now');
    expect(linkMarkdownPaths('```\n`a.md`\n```'), '```\n`a.md`\n```');
    expect(linkMarkdownPaths('[`a.md`](a.md)'), '[`a.md`](a.md)');
  });

  test('links bare paths in plain text', () {
    expect(linkMarkdownPaths('See docs/plan.md.'), 'See [docs/plan.md](docs/plan.md).');
    expect(linkMarkdownPaths(r'Wrote C:\work\x.md, (and /srv/y.MD)'), r'Wrote [C:\\work\\x.md](C:/work/x.md), (and [/srv/y.MD](/srv/y.MD))');
    expect(linkMarkdownPaths('"C:/a b/c.md" a.md\n\tb.md'), '"C:/a b/c.md" [a.md](a.md)\n\t[b.md](b.md)');
    const kept = '`see b.md` [the c.md notes](c.md) https://x.io/d.md e.md\'s\n```\nf.md\n```';
    expect(linkMarkdownPaths(kept), kept);
    expect(linkMarkdownPaths('```\nx\n```\ng.md'), '```\nx\n```\n[g.md](g.md)');
  });

  test('scopes serialise as the protocol expects', () {
    expect(const FileScopeProject(projectId: 'p').toJson(), {'kind': 'project', 'project_id': 'p'});
    expect(const FileScopeTask(taskId: 't').toJson(), {'kind': 'task', 'task_id': 't'});
  });

  test('parses a file list', () {
    final files = MarkdownFileList.fromJson({
      'root': '/w',
      'files': [
        {'path': 'docs/a.md', 'byte_length': 12, 'modified_ms': 5},
      ],
      'truncated': true,
    });
    expect(files.truncated, isTrue);
    expect(files.files.single.folder, 'docs');
    expect(files.files.single.name, 'a.md');
  });
}
