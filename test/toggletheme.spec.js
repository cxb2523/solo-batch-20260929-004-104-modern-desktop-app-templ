import { By } from 'selenium-webdriver';
import { driver } from './test.js';
import { expect } from 'chai';

describe('Home', () => {
  it('successfully loads ', () => { });

  it('allows toggling the theme', async () => {
    const header = await driver.findElement(By.css('.mantine-AppShell-header'));
    const bgBeforeToggle = await header.getCssValue('background-color');
    await driver.findElement(By.css('#toggle-theme')).click();
    // theme_set round-trips through the backend contract before the event
    // re-applies the scheme, so wait for the header to actually change.
    await driver.wait(async () =>
      (await header.getCssValue('background-color')) !== bgBeforeToggle
    , 5_000);
  }).timeout(10_000);
});
