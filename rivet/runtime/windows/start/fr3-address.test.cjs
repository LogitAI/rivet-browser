const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const html = fs.readFileSync(__dirname + '/start.html', 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const element = () => ({addEventListener() {}, classList: {toggle() {}}, value: '', textContent: ''});
const context = { URLSearchParams, TextEncoder, location: {search: ''}, document: {querySelector: element, documentElement: {style: {setProperty() {}}}} };
vm.runInNewContext(script, context);
function fr3_start_page_takes_its_spaces_colours_and_refuses_anything_else() {
  const set = {};
  const page = { URLSearchParams, TextEncoder, location: {search: '?tint=5b8bab&accent=red;x'},
    document: {querySelector: element, documentElement: {style: {setProperty(name, value) { set[name] = value; }}}} };
  vm.runInNewContext(script, page);
  assert.deepEqual(set, {'--tint': '#5b8bab'});
}
fr3_start_page_takes_its_spaces_colours_and_refuses_anything_else();
console.log('fr3_start_page_takes_its_spaces_colours_and_refuses_anything_else passed');
function fr3_address_rules() {
  for (const [input, expected] of [
    [' https://a.test/x ', 'https://a.test/x'], ['about:blank', 'about:blank'],
    ['example.com', 'https://example.com'], ['localhost:8080', 'https://localhost:8080'],
    ['rivet browser', 'https://www.google.com/search?q=rivet+browser'],
    ['한글 & a', 'https://www.google.com/search?q=%ED%95%9C%EA%B8%80+%26+a'],
    ['a\tb', 'https://www.google.com/search?q=a%09b']]) {
    assert.equal(context.addressFor(input), expected);
  }
}
fr3_address_rules();
console.log('fr3_address_rules passed');
