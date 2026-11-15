import { expect, test } from 'vitest';
import { createApp } from 'vue';
import ImportComparison from './components/ImportComparison.vue';

test('comparison shows supplied fields and readable senses, examples and grammar', () => {
  const host = document.createElement('div');
  const app = createApp(ImportComparison, {
    incoming: { kanji: '猫', kana: 'ねこ', note: '', parts_of_speech: ['noun'], senses: [{ meaning: '猫科动物' }], examples: [{ jp: '猫がいる', cn: '有猫', sense_index: 0 }] },
    existing: { kanji: '猫', kana: 'ねこ', note: '只保存在本地的备注', parts_of_speech: ['noun'], senses: [{ meaning: '猫' }], examples: [] },
    present: ['kanji', 'kana', 'parts_of_speech', 'senses', 'examples'],
  });
  app.mount(host);
  expect(host.textContent).toContain('当前词条');
  expect(host.textContent).toContain('文件内容');
  expect(host.textContent).toContain('名词');
  expect(host.textContent).toContain('1. 猫科动物');
  expect(host.textContent).toContain('关联义项 1');
  expect(host.textContent).not.toContain('只保存在本地的备注');
  expect(host.querySelectorAll('.changed')).toHaveLength(2);
  app.unmount();
});
