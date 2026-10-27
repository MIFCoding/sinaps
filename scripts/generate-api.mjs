import { execFileSync } from 'node:child_process';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import openapiTS, { astToString } from 'openapi-typescript';
import ts from 'typescript';

const check = process.argv.includes('--check');
const raw = execFileSync(
  'cargo',
  ['run', '--quiet', '--locked', '-p', 'synapse-api', '--', 'openapi'],
  {
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  },
);
const document = JSON.parse(raw);
const canonical = (value) =>
  Array.isArray(value)
    ? value.map(canonical)
    : value && typeof value === 'object'
      ? Object.fromEntries(
          Object.keys(value)
            .sort()
            .map((key) => [key, canonical(value[key])]),
        )
      : value;
const endpoints = {};
for (const [path, item] of Object.entries(document.paths)) {
  for (const [method, operation] of Object.entries(item)) {
    if (!['get', 'post', 'patch', 'put', 'delete', 'head', 'options'].includes(method)) continue;
    if (!operation.operationId || endpoints[operation.operationId])
      throw new Error('Операция не имеет уникального operationId');
    endpoints[operation.operationId] = { method: method.toUpperCase(), path };
  }
}
const ast = await openapiTS(document);
// Удаляем служебные англоязычные комментарии генератора, оставляя русский заголовок.
const printed = astToString(ast, { printerOptions: { removeComments: true } });
const source = ts.createSourceFile('api.ts', printed, ts.ScriptTarget.Latest, true);
const generated = ts.createPrinter({ removeComments: true }).printFile(source);
const header =
  '// Создано автоматически из серверного OpenAPI. Обновление: npm run api:generate.\n';
const outputs = {
  'contracts/openapi.json': JSON.stringify(canonical(document), null, 2) + '\n',
  'apps/frontend/src/lib/generated/api.ts': header + generated,
  'apps/frontend/src/lib/generated/endpoints.ts':
    header +
    'export const endpoints = ' +
    JSON.stringify(canonical(endpoints), null, 2) +
    ' as const;\n',
};
for (const [path, expected] of Object.entries(outputs)) {
  if (check) {
    const actual = await readFile(path, 'utf8').catch(() => '');
    if (actual !== expected)
      throw new Error(`Устарел ${path}. Выполните npm run api:generate и сохраните результат.`);
  } else {
    await mkdir(path.slice(0, path.lastIndexOf('/')), { recursive: true });
    await writeFile(path, expected, 'utf8');
  }
}
console.log(check ? 'Контракт и типы актуальны.' : 'OpenAPI, TypeScript и операции API обновлены.');
