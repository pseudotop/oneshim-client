/**
 * 클래스명 병합 유틸리티
 *
 * clsx로 조건부 클래스 조합 + tailwind-merge로 충돌 해결
 */
import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}
