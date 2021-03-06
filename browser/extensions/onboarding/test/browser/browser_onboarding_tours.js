/* Any copyright is dedicated to the Public Domain.
 * http://creativecommons.org/publicdomain/zero/1.0/ */

 "use strict";

requestLongerTimeout(2);

function assertOnboardingDestroyed(browser) {
  return ContentTask.spawn(browser, {}, function() {
    let expectedRemovals = [
      "#onboarding-overlay",
      "#onboarding-overlay-button"
    ];
    for (let selector of expectedRemovals) {
      let removal = content.document.querySelector(selector);
      ok(!removal, `Should remove ${selector} onboarding element`);
    }
  });
}

function assertTourCompleted(tourId, expectComplete, browser) {
  return ContentTask.spawn(browser, { tourId, expectComplete }, function(args) {
    let item = content.document.querySelector(`#${args.tourId}.onboarding-tour-item`);
    let completedTextId = `onboarding-complete-${args.tourId}-text`;
    let completedText = item.querySelector(`#${completedTextId}`);
    if (args.expectComplete) {
      ok(item.classList.contains("onboarding-complete"), `Should set the complete #${args.tourId} tour with the complete style`);
      ok(completedText, "Text label should be present for a completed item");
      is(completedText.id, completedTextId, "Text label node should have a unique id");
      ok(completedText.getAttribute("aria-label"), "Text label node should have an aria-label attribute set");
      is(item.getAttribute("aria-describedby"), completedTextId,
        "Completed item should have aria-describedby attribute set to text label node's id");
    } else {
      ok(!item.classList.contains("onboarding-complete"), `Should not set the incomplete #${args.tourId} tour with the complete style`);
      ok(!completedText, "Text label should not be present for an incomplete item");
      ok(!item.hasAttribute("aria-describedby"), "Incomplete item should not have aria-describedby attribute set");
    }
  });
}

add_task(async function test_hide_onboarding_tours() {
  resetOnboardingDefaultState();

  let tourIds = TOUR_IDs;
  let tabs = [];
  for (let url of URLs) {
    let tab = await openTab(url);
    await promiseOnboardingOverlayLoaded(tab.linkedBrowser);
    await BrowserTestUtils.synthesizeMouseAtCenter("#onboarding-overlay-button", {}, tab.linkedBrowser);
    await promiseOnboardingOverlayOpened(tab.linkedBrowser);
    tabs.push(tab);
  }

  let expectedPrefUpdates = [
    promisePrefUpdated("browser.onboarding.hidden", true),
    promisePrefUpdated("browser.onboarding.notification.finished", true)
  ];
  tourIds.forEach(id => expectedPrefUpdates.push(promisePrefUpdated(`browser.onboarding.tour.${id}.completed`, true)));

  await BrowserTestUtils.synthesizeMouseAtCenter("#onboarding-tour-hidden-checkbox", {}, gBrowser.selectedBrowser);
  await BrowserTestUtils.synthesizeMouseAtCenter("#onboarding-overlay-close-btn", {}, gBrowser.selectedBrowser);
  await Promise.all(expectedPrefUpdates);

  for (let i = tabs.length - 1; i >= 0; --i) {
    let tab = tabs[i];
    await assertOnboardingDestroyed(tab.linkedBrowser);
    await BrowserTestUtils.removeTab(tab);
  }
});

add_task(async function test_click_action_button_to_set_tour_completed() {
  resetOnboardingDefaultState();

  let tourIds = TOUR_IDs;
  let tabs = [];
  for (let url of URLs) {
    let tab = await openTab(url);
    await promiseOnboardingOverlayLoaded(tab.linkedBrowser);
    await BrowserTestUtils.synthesizeMouseAtCenter("#onboarding-overlay-button", {}, tab.linkedBrowser);
    await promiseOnboardingOverlayOpened(tab.linkedBrowser);
    tabs.push(tab);
  }

  let completedTourId = tourIds[0];
  let expectedPrefUpdate = promisePrefUpdated(`browser.onboarding.tour.${completedTourId}.completed`, true);
  await BrowserTestUtils.synthesizeMouseAtCenter(`#${completedTourId}-page .onboarding-tour-action-button`, {}, gBrowser.selectedBrowser);
  await expectedPrefUpdate;

  for (let i = tabs.length - 1; i >= 0; --i) {
    let tab = tabs[i];
    await assertOverlaySemantics(tab.linkedBrowser);
    for (let id of tourIds) {
      await assertTourCompleted(id, id == completedTourId, tab.linkedBrowser);
    }
    await BrowserTestUtils.removeTab(tab);
  }
});

add_task(async function test_set_right_tour_completed_style_on_overlay() {
  resetOnboardingDefaultState();

  let tourIds = TOUR_IDs;
  // Make the tours of even number as completed
  for (let i = 0; i < tourIds.length; ++i) {
    setTourCompletedState(tourIds[i], i % 2 == 0);
  }

  let tabs = [];
  for (let url of URLs) {
    let tab = await openTab(url);
    await promiseOnboardingOverlayLoaded(tab.linkedBrowser);
    await BrowserTestUtils.synthesizeMouseAtCenter("#onboarding-overlay-button", {}, tab.linkedBrowser);
    await promiseOnboardingOverlayOpened(tab.linkedBrowser);
    tabs.push(tab);
  }

  for (let i = tabs.length - 1; i >= 0; --i) {
    let tab = tabs[i];
    await assertOverlaySemantics(tab.linkedBrowser);
    for (let j = 0; j < tourIds.length; ++j) {
      await assertTourCompleted(tourIds[j], j % 2 == 0, tab.linkedBrowser);
    }
    await BrowserTestUtils.removeTab(tab);
  }
});
