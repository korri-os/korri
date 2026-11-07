# Every part: native render verdicts

Run: `/tmp/slint-caliper-render-all/check-1794016968831843113-570445/report.json`.
Every exported state ran on rg35xxsp and odin2portal. Counts below include both devices.
All 540 frames are Rendered and both samples match. Empty=0, Failed frame=0.
The full report contains the per-state/device reasons and PNG paths. Review is not a pass.

| Part (under clients/shell/packages/) | Exported states | Rendered | spill Review | accessibility Failed | focus Failed |
|---|---|---:|---:|---:|---:|
| `pico/pages/PicoGameDetail.page.preview.slint` | `ChooseLocation`, `ConfirmRemoval`, `Default` | 6 | 0 | 0 | 0 |
| `pico/pages/PicoHome.page.preview.slint` | `Default`, `Grid`, `Hero`, `MenuOpen`, `PlayingNow`, `Problem`, `Starting`, `StartingSteps` | 16 | 0 | 0 | 0 |
| `pico/pages/PicoLaunchSettings.page.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/pages/PicoLibrary.page.preview.slint` | `Default`, `NoResults` | 4 | 0 | 0 | 0 |
| `pico/pages/PicoOnboarding.page.preview.slint` | `Done`, `Editor`, `Hello`, `Wifi` | 8 | 0 | 0 | 0 |
| `pico/pages/PicoOverlay.page.preview.slint` | `ConfirmQuit`, `Default` | 4 | 0 | 0 | 0 |
| `pico/pages/PicoRunnerPicker.page.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/pages/PicoSettings.page.preview.slint` | `Default`, `Editor`, `Identity`, `Modal` | 8 | 0 | 0 | 0 |
| `pico/pages/PicoSettingsPanel.page.preview.slint` | `Default`, `Wide` | 4 | 0 | 0 | 0 |
| `pico/pages/PicoShowcase.page.preview.slint` | `Default`, `Idle` | 4 | 0 | 0 | 0 |
| `pico/pages/PicoStore.page.preview.slint` | `Entry`, `Providers`, `Results` | 6 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoBadge.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoBusyMeter.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoButton.atom.preview.slint` | `Default`, `NoMark` | 4 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoChip.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoClock.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoCoverArt.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoDownloadBar.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoHint.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoKey.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoLine.atom.preview.slint` | `Default`, `Long` | 4 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoMenuKey.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoPaletteBar.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoPara.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoPixelDisc.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoPixelLock.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoRail.atom.preview.slint` | `Default` | 2 | 1 | 0 | 0 |
| `pico/ui/atoms/PicoReveal.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoRow.atom.preview.slint` | `Default`, `OnCard` | 4 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoScroll.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoSegments.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoSignalBars.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoSlider.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoStat.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoStepTrack.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoSub.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoTally.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/atoms/PicoTitle.atom.preview.slint` | `Default`, `Long` | 4 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoButtonBar.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoCard.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoCart.molecule.preview.slint` | `Default`, `Still` | 4 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoChoiceRow.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoControlRow.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoControllerRow.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoEmptySlot.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoGameFacts.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoInviteNotice.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoInviteRow.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoKeyboard.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoNetworkMarks.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoNetworkRow.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoNotice.molecule.preview.slint` | `Ask`, `Report`, `Warn` | 6 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoQueryField.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoResultRow.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoScopeTabs.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoSettingRow.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoSetupHeading.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoStarterPack.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoStatRun.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoStatusBar.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoStreamPlayers.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoTabs.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/molecules/PicoTextField.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoAttract.organism.preview.slint` | `Default`, `EmptyLibrary` | 4 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoCartGrid.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoCartShelf.organism.preview.slint` | `Default`, `Empty` | 4 | 1 | 0 | 0 |
| `pico/ui/organisms/PicoChoicePicker.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoControllerCheck.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoGameActions.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoGameDetail.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoGameHero.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoGameStage.organism.preview.slint` | `Default`, `LongTitle` | 4 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoGamesFound.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoIdentityDialog.organism.preview.slint` | `Backup`, `Default` | 4 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoIdentitySetup.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoInviteNotices.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoLaunchStage.organism.preview.slint` | `Downloading`, `Starting`, `Steps` | 6 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoLibraryBrowser.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoLocationPicker.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoMenu.organism.preview.slint` | `Default`, `Short` | 4 | 2 | 0 | 1 |
| `pico/ui/organisms/PicoModal.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoPauseMenu.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoPlayingNow.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoResumeList.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoSetupDone.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoSetupWelcome.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoStoreEntry.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoStoreProviders.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoStoreResults.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoTextEditor.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/organisms/PicoWifiSetup.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/templates/PicoDisplays.template.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/templates/PicoGameOverlay.template.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `pico/ui/templates/PicoHomeLayout.template.preview.slint` | `Card`, `Grid`, `Hero`, `PlayingNow`, `Shelf` | 10 | 3 | 0 | 0 |
| `pico/ui/templates/PicoPanelScreen.template.preview.slint` | `Beside`, `Default` | 4 | 0 | 0 | 0 |
| `pico/ui/templates/PicoScreenShell.template.preview.slint` | `Breadcrumb`, `Default`, `Track` | 6 | 0 | 0 | 0 |
| `zone/pages/ZoneGames.page.preview.slint` | `Empty`, `Find`, `Loading`, `Modern`, `Ready`, `Retro` | 12 | 12 | 6 | 0 |
| `zone/pages/ZoneLaunchSettings.page.preview.slint` | `Ready` | 2 | 2 | 0 | 0 |
| `zone/pages/ZoneOnboarding.page.preview.slint` | `Controllers`, `Identity` | 4 | 4 | 0 | 0 |
| `zone/pages/ZoneOptions.page.preview.slint` | `Default`, `Keyboard` | 4 | 4 | 4 | 2 |
| `zone/pages/ZonePaused.page.preview.slint` | `Default`, `Question` | 4 | 0 | 2 | 0 |
| `zone/pages/ZoneRunWith.page.preview.slint` | `Ready` | 2 | 2 | 0 | 0 |
| `zone/pages/ZoneSaveData.page.preview.slint` | `Backup`, `Band` | 4 | 4 | 0 | 0 |
| `zone/pages/ZoneStore.page.preview.slint` | `Results` | 2 | 2 | 0 | 0 |
| `zone/ui/atoms/ZoneBanner.atom.preview.slint` | `Default`, `Long` | 4 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneBars.atom.preview.slint` | `Offline`, `Three` | 4 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneButton.atom.preview.slint` | `Default`, `Destructive`, `Disabled`, `Focused`, `Lit` | 10 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneCaps.atom.preview.slint` | `Danger`, `Fact`, `Hud`, `KorriName`, `Modern`, `Retro`, `White` | 14 | 1 | 0 | 0 |
| `zone/ui/atoms/ZoneCoverWell.atom.preview.slint` | `NoArt`, `Tiny` | 4 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneEmblem.atom.preview.slint` | `Key`, `Link`, `Pad`, `Restore`, `Retired` | 10 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneKeycap.atom.preview.slint` | `BackWord`, `KeyOnly` | 4 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneMeasure.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `zone/ui/atoms/ZonePanel.atom.preview.slint` | `Blue`, `Default` | 4 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneQrWell.atom.preview.slint` | `Default` | 2 | 2 | 0 | 0 |
| `zone/ui/atoms/ZoneShape.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneSignal.atom.preview.slint` | `Cut`, `Wifi` | 4 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneTab.atom.preview.slint` | `Active`, `Focused` | 4 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneTag.atom.preview.slint` | `Continue`, `Download`, `Here`, `Remote`, `System` | 10 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneTriangle.atom.preview.slint` | `LeftSteel`, `Right` | 4 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneUnderline.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneVeil.atom.preview.slint` | `Default`, `Modern` | 4 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneVersus.atom.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `zone/ui/atoms/ZoneWords.atom.preview.slint` | `LongTitle`, `LongWord` | 4 | 2 | 0 | 0 |
| `zone/ui/molecules/ZoneActionRow.molecule.preview.slint` | `Two`, `Wraps` | 4 | 1 | 0 | 1 |
| `zone/ui/molecules/ZoneBusyStrip.molecule.preview.slint` | `Default`, `Progress`, `Steps` | 6 | 0 | 0 | 0 |
| `zone/ui/molecules/ZoneCard.molecule.preview.slint` | `Focused`, `SideLongTitle` | 4 | 0 | 0 | 0 |
| `zone/ui/molecules/ZoneCardBand.molecule.preview.slint` | `Default`, `Last`, `OneCard` | 6 | 6 | 6 | 0 |
| `zone/ui/molecules/ZoneDialog.molecule.preview.slint` | `OneButton`, `Question` | 4 | 0 | 0 | 0 |
| `zone/ui/molecules/ZoneFactLine.molecule.preview.slint` | `Default`, `NoneRecorded` | 4 | 0 | 0 | 0 |
| `zone/ui/molecules/ZoneFoot.molecule.preview.slint` | `Offline`, `Wifi` | 4 | 4 | 0 | 0 |
| `zone/ui/molecules/ZoneKeyboard.molecule.preview.slint` | `Find`, `Secret` | 4 | 0 | 0 | 0 |
| `zone/ui/molecules/ZoneOptionGroup.molecule.preview.slint` | `Caption`, `Default` | 4 | 0 | 0 | 0 |
| `zone/ui/molecules/ZoneOptionRow.molecule.preview.slint` | `Choice`, `Disabled`, `Focused`, `Network` | 8 | 0 | 0 | 0 |
| `zone/ui/molecules/ZoneSaveCard.molecule.preview.slint` | `New`, `Retired` | 4 | 0 | 0 | 0 |
| `zone/ui/molecules/ZoneSectionCard.molecule.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `zone/ui/molecules/ZoneTabStrip.molecule.preview.slint` | `Banner`, `FocusedTab`, `Games` | 6 | 2 | 0 | 0 |
| `zone/ui/molecules/ZoneTagRow.molecule.preview.slint` | `Card`, `Natural` | 4 | 0 | 0 | 0 |
| `zone/ui/molecules/ZoneTitleCard.molecule.preview.slint` | `Default`, `LongTitle` | 4 | 0 | 0 | 0 |
| `zone/ui/organisms/ZoneAttract.organism.preview.slint` | `Stage`, `TitleTurn` | 4 | 4 | 0 | 0 |
| `zone/ui/organisms/ZoneControllerCard.organism.preview.slint` | `Heard`, `Waiting` | 4 | 0 | 2 | 0 |
| `zone/ui/organisms/ZoneGameOpts.organism.preview.slint` | `Default` | 2 | 2 | 0 | 0 |
| `zone/ui/organisms/ZoneGround.organism.preview.slint` | `Games`, `Options`, `Paused`, `Savedata`, `Titlecard` | 10 | 10 | 0 | 0 |
| `zone/ui/organisms/ZoneLaunchCard.organism.preview.slint` | `Default` | 2 | 2 | 0 | 0 |
| `zone/ui/organisms/ZoneOptionsEntry.organism.preview.slint` | `Default` | 2 | 0 | 0 | 0 |
| `zone/ui/organisms/ZonePausedControl.organism.preview.slint` | `Choice`, `Range` | 4 | 0 | 0 | 0 |
| `zone/ui/organisms/ZoneRowList.organism.preview.slint` | `Options` | 2 | 1 | 2 | 1 |
| `zone/ui/organisms/ZoneRunWithRoute.organism.preview.slint` | `Saved` | 2 | 0 | 0 | 0 |
| `zone/ui/organisms/ZoneSaveBand.organism.preview.slint` | `Default`, `Retired` | 4 | 2 | 0 | 0 |
| `zone/ui/organisms/ZoneSecretSlab.organism.preview.slint` | `Focused` | 2 | 0 | 0 | 0 |
| `zone/ui/organisms/ZoneSkyBand.organism.preview.slint` | `Entry`, `Head` | 4 | 4 | 2 | 1 |
| `zone/ui/organisms/ZoneStatusLayer.organism.preview.slint` | `Busy`, `Problem`, `ProblemLongReason`, `Progress`, `Running`, `Steps` | 12 | 0 | 0 | 0 |
| `zone/ui/organisms/ZoneStepStrip.organism.preview.slint` | `Second` | 2 | 0 | 0 | 0 |
| `zone/ui/organisms/ZoneTally.organism.preview.slint` | `Counted` | 2 | 0 | 0 | 0 |
| `zone/ui/organisms/ZoneWarningLines.organism.preview.slint` | `Message`, `Warning` | 4 | 0 | 0 | 0 |
| `zone/ui/templates/ZoneShell.template.preview.slint` | `Banner`, `Steps`, `Tabs` | 6 | 6 | 0 | 0 |
