import ts from 'typescript';
import { expect, it } from 'vitest';

it('keeps Rust routes, the method contract, and every real frontend dispatch call aligned', () => {
  const config = ts.readConfigFile('tsconfig.json', ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, ts.sys.getCurrentDirectory());
  const program = ts.createProgram(parsed.fileNames, parsed.options);
  const checker = program.getTypeChecker();
  const source = program.getSourceFile(ts.sys.resolvePath('src/contracts.ts'))!;
  const contract = source.statements.find((node): node is ts.InterfaceDeclaration => ts.isInterfaceDeclaration(node) && node.name.text === 'Contracts')!;
  const methods = contract.members.map(member => (member.name as ts.StringLiteral).text).sort();
  const rust = ts.sys.readFile('crates/core/src/lib.rs')!.split('fn dispatch_inner(')[1].split('_ => bail!')[0];
  const routes = [...rust.matchAll(/"([a-z][a-zA-Z.]*)"(?=\s*(?:\||=>))/g)].map(match => match[1]).sort();
  expect(methods).toEqual(routes);

  const called = new Set<string>();
  const literals = (type: ts.Type): string[] => {
    if (type.isUnion()) return type.types.flatMap(literals);
    expect(type.isStringLiteral(), `A frontend method widened to ${checker.typeToString(type)}`).toBe(true);
    return type.isStringLiteral() ? [type.value] : [];
  };
  const tupleMethods = (type: ts.Type): string[] => {
    if (type.isUnion()) return type.types.flatMap(tupleMethods);
    expect(checker.isTupleType(type), 'Spread dispatch arguments must remain a correlated tuple').toBe(true);
    return literals(checker.getTypeArguments(type as ts.TypeReference)[0]);
  };
  for (const file of program.getSourceFiles()) {
    if (!parsed.fileNames.some(name => ts.sys.resolvePath(name) === ts.sys.resolvePath(file.fileName)) || /\.(test|typecheck)\./.test(file.fileName)) continue;
    const visit = (node: ts.Node) => {
      if (ts.isCallExpression(node) && (
        ts.isIdentifier(node.expression) && node.expression.text === 'dispatch' ||
        ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'dispatch'
      )) {
        const argument = node.arguments[0];
        const names = ts.isSpreadElement(argument)
          ? tupleMethods(checker.getTypeAtLocation(argument.expression))
          : literals(checker.getTypeAtLocation(argument));
        for (const name of names) {
          expect(methods, `${file.fileName}: ${name}`).toContain(name);
          called.add(name);
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(file);
  }
  // Prevent declared methods with no actual frontend consumer from silently accumulating.
  expect([...called].sort()).toEqual(methods);
}, 15_000);
