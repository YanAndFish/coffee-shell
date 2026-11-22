// The component gallery as an app entry, for the tests (`apps/tui/Gallery.test.tsx` opens it
// with `open('./src/ui/gallery.entry.tsx', …)`). `UiGallery` is the Rust root
// `coffee_app::ui::gallery::UiGallery`, registered next to `App` in `src/lib.rs`.
import { UiGallery } from 'coffee-app/native'
import { mount } from 'nsh'

function Gallery() {
  return <UiGallery />
}

mount(Gallery)
