import { describe, expect, it } from 'vitest';
import { parseSkillSourceInput, selectCommandSkills } from './skillSourceInput';

describe('Skills CLI source input', () => {
  it('keeps a complete Kami bundle subpath for plain addresses and npx commands', () => {
    const source = { kind: 'git', locator: 'https://github.com/tw93/kami', subpath: 'plugins/kami/skills/kami' };
    expect(parseSkillSourceInput('tw93/kami/plugins/kami/skills/kami').source).toEqual(source);
    expect(parseSkillSourceInput('npx skills add tw93/kami/plugins/kami/skills/kami -a universal -g -y').source).toEqual(source);
    expect(() => parseSkillSourceInput('tw93/kami/../private')).toThrow();
  });
  it('extracts the repository from the WeRead installation command', () => {
    expect(parseSkillSourceInput('npx skills add Tencent/WeChatReading -g')).toEqual({
      source: { kind: 'git', locator: 'https://github.com/Tencent/WeChatReading' }, skillNames: [], fromCommand: true,
    });
  });
  it('keeps named Skill selections and accepts installer flags without running them', () => {
    const parsed = parseSkillSourceInput('npx -y skills@latest add "https://github.com/acme/skills" --skill writing --agent codex claude --global --yes');
    expect(parsed.skillNames).toEqual(['writing']);
    expect(selectCommandSkills([{ name: 'writing', subpath: 'skills/writing' }, { name: 'other', subpath: 'other' }], parsed.skillNames)).toEqual([{ name: 'writing', subpath: 'skills/writing' }]);
    expect(() => selectCommandSkills([{ name: 'other', subpath: 'other' }], parsed.skillNames)).toThrow('writing');
  });
  it.each(['npx skills add acme/skills -g && whoami', 'npx skills add acme/skills; whoami', 'npx skills add acme/skills --unknown', 'npx other install acme/skills', 'npx skills add acme/skills --skill', 'npx skills add "acme/skills'])('rejects unsupported or ambiguous input: %s', input => {
    expect(() => parseSkillSourceInput(input)).toThrow();
  });
});
