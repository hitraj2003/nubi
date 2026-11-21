; Set System.AppUserModel.ID on the link using its IPropertyStore.
!macro SetNubiShortcutIdentity shortcut
  System::Call 'ole32::CoInitialize(p0)'
  System::Call 'ole32::CoCreateInstance(g "{00021401-0000-0000-C000-000000000046}", p0, i1, g "{000214F9-0000-0000-C000-000000000046}", *p.r0) i.r9'
  ${If} $9 == 0
    System::Call '$0->0(g "{0000010B-0000-0000-C000-000000000046}", *p.r1) i.r9'
    ${If} $9 == 0
      System::Call '$1->5(w "${shortcut}", i2) i.r9'
      ${If} $9 == 0
        System::Call '$0->0(g "{886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99}", *p.r2) i.r9'
        ${If} $9 == 0
          System::Call 'oleaut32::SysAllocString(w "dev.nubi.app") p.r3'
          System::Call '*(g "{9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}", i5) p.r4'
          System::Call '*(&i2 8, &i2 0, &i2 0, &i2 0, p r3, p0) p.r5'
          System::Call '$2->6(p r4, p r5) i.r9'
          ${If} $9 == 0
            System::Call '$2->7() i.r9'
            System::Call '$1->6(w "${shortcut}", i1) i.r9'
          ${EndIf}
          System::Free $4
          System::Free $5
          System::Call 'oleaut32::SysFreeString(p r3)'
          System::Call '$2->2()'
        ${EndIf}
      ${EndIf}
      System::Call '$1->2()'
    ${EndIf}
    System::Call '$0->2()'
  ${EndIf}
  System::Call 'ole32::CoUninitialize()'
!macroend