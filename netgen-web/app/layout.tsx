import type { Metadata } from 'next'
import { Inter, JetBrains_Mono } from 'next/font/google'
import { AppProvider } from '@/lib/store'
import './globals.css'

const inter = Inter({ subsets: ['latin'], variable: '--font-inter' })
const jetbrainsMono = JetBrains_Mono({ subsets: ['latin'], variable: '--font-mono' })

export const metadata: Metadata = {
  title: 'Labyrinth — From intent to infrastructure',
  description: 'Network design compiler. Natural language to verified topologies, configs, and GNS3 labs.',
}

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${inter.variable} ${jetbrainsMono.variable}`}>
      <body className="bg-bg-page text-text-primary antialiased font-sans">
        <AppProvider>{children}</AppProvider>
      </body>
    </html>
  )
}
