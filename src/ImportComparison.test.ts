import { expect, test } from 'vitest';
import { createApp } from 'vue';
import ImportComparison from './components/ImportComparison.vue';

test('comparison shows supplied fields and readable senses, examples and grammar', () => {
  const host = document.createElement('div');
  const app = createApp(ImportComparison, {
    incoming: { kanji: '猫', kana: 'ねこ', note: '', parts_of_speech: ['noun'], meaning: '猫科动物', examples_jp: '猫がいる', examples_cn: '有猫' },
    existing: { kanji: '猫', kana: 'ねこ', note: '只保存在本地的备注', parts_of_speech: ['noun'], meaning: '猫', examples_jp: '', examples_cn: '' },
    present: ['kanji', 'kana', 'parts_of_speech', 'meaning', 'examples_jp', 'examples_cn'],
  });
  app.mount(host);
  expect(host.textContent).toContain('当前词条');
  expect(host.textContent).toContain('文件内容');
  expect(host.textContent).toContain('名词');
  expect(host.textContent).toContain('猫科动物');
  expect(host.textContent).toContain('有猫');
  expect(host.textContent).not.toContain('只保存在本地的备注');
  expect(host.querySelectorAll('.changed')).toHaveLength(3);
  app.unmount();
});
